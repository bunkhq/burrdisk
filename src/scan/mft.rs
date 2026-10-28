//! Scans an NTFS volume by reading its master file table (MFT) directly instead of
//! listing every folder, which takes a second or two where a walk takes many.
//!
//! Raw access to a volume needs administrator rights, so only the elevated helper
//! tries this. Anything unexpected (another file system, a heavily fragmented MFT,
//! a failed read) returns `Unsupported` and the caller walks the folders instead.
//! Sizes follow the same rules as the walk: a file's length, or what it really uses
//! when it is compressed, sparse or a cloud placeholder; hard links count once per
//! name; symlinks and junctions are not entered. The one difference is files compressed
//! by Windows itself (the "Compact OS" kind), which a folder listing hides the marks of:
//! the walk counts their full length, this counts what they take on disk.

use std::fs::File;
use std::io;
use std::os::windows::fs::FileExt;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::{Child, Dir, Progress, Tree, flatten};

pub enum Failure {
    Cancelled,
    /// The volume can't be read this way; scan it the ordinary way.
    Unsupported,
}

/// Records handled per task: about 8 MB of file table at the usual 1 KB a record.
const CHUNK: usize = 8192;

const ROOT_RECORD: u32 = 5;
/// Records below this are NTFS's own bookkeeping files ($MFT, $LogFile, ...), which a
/// folder listing never shows.
const FIRST_USER_RECORD: u32 = 24;
const REF_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

const STANDARD_INFORMATION: u32 = 0x10;
const FILE_NAME: u32 = 0x30;
const DATA: u32 = 0x80;
const REPARSE_POINT: u32 = 0xC0;
const END: u32 = 0xFFFF_FFFF;

const NAMESPACE_DOS: u8 = 2;
const ATTR_COMPRESSED: u16 = 0x0001;
const ATTR_SPARSE: u16 = 0x8000;
/// The reparse point bit of a name's file flags.
const ATTR_REPARSE: u32 = 0x400;
const REPARSE_SYMLINK: u32 = 0xA000_000C;
const REPARSE_MOUNT_POINT: u32 = 0xA000_0003;
/// Windows' own file compression keeps a file's data in a stream of this name, leaving the
/// main one empty.
const REPARSE_WOF: u32 = 0x8000_0017;
const WOF_STREAM: &str = "WofCompressedData";
/// File attributes whose real size on disk differs from the file's length: the same
/// set the walk asks NTFS about.
const SPECIAL_ATTRIBUTES: u32 = 0x0800 // compressed
    | 0x1000 // offline
    | 0x0200 // sparse
    | 0x0004_0000 // recall on open
    | 0x0040_0000; // recall on data access

// Per-record flags.
const IN_USE: u8 = 1;
const DIR: u8 = 2;
const SPECIAL: u8 = 4;
const HAS_DATA: u8 = 8;
const WOF: u8 = 16;

const NO_PACKED: u64 = u64::MAX;

fn le<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
    bytes
        .get(at..at + N)
        .and_then(|b| b.try_into().ok())
        .unwrap_or([0; N])
}
fn u16at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(le(bytes, at))
}
fn u32at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(le(bytes, at))
}
fn u64at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(le(bytes, at))
}

/// Reads that bypass the file cache, which is what lets a volume be read at the speed
/// of the drive; they need their buffers aligned to the sector size.
fn open_unbuffered(device: &str) -> Option<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
    std::fs::OpenOptions::new().read(true).custom_flags(FILE_FLAG_NO_BUFFERING).open(device).ok()
}

struct Aligned {
    ptr: *mut u8,
    layout: std::alloc::Layout,
}

// The buffer is owned outright and only ever used by one thread at a time.
unsafe impl Send for Aligned {}

impl Aligned {
    fn new(len: usize) -> Self {
        let layout = std::alloc::Layout::from_size_align(len, 4096).expect("buffer layout");
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Self { ptr, layout }
    }

    fn bytes(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }
}

impl Drop for Aligned {
    fn drop(&mut self) {
        unsafe { std::alloc::dealloc(self.ptr, self.layout) }
    }
}

/// A stretch of the file table's data: where it sits in the table and on the volume.
struct Extent {
    start: u64,
    disk: u64,
    len: u64,
}

struct Layout {
    cluster: u64,
    record: usize,
    mft_lcn: u64,
}

fn read_layout(vol: &File) -> Option<Layout> {
    let mut boot = [0u8; 4096];
    read_at(vol, &mut boot, 0).ok()?;
    if &boot[3..11] != b"NTFS    " {
        return None;
    }
    let sector = u64::from(u16at(&boot, 0x0B));
    let per_cluster = match boot[0x0D] {
        n @ 1..=0x80 => u64::from(n),
        n if n >= 0xE0 => 1u64 << (256 - u32::from(n)),
        _ => return None,
    };
    let cluster = sector * per_cluster;
    let record = match boot[0x40] as i8 {
        n @ 1.. => u64::from(n as u8) * cluster,
        n @ -31..=-9 => 1u64 << -n,
        _ => return None,
    };
    if !(512..=4096).contains(&sector) || !(512..=65536).contains(&record) || record < sector {
        return None;
    }
    Some(Layout { cluster, record: record as usize, mft_lcn: u64at(&boot, 0x30) })
}

fn read_at(vol: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    while !buf.is_empty() {
        let n = vol.seek_read(buf, offset)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf = &mut buf[n..];
        offset += n as u64;
    }
    Ok(())
}

/// Reads the file table's bytes `[offset, offset + buf.len())`, which may span extents.
fn read_table(vol: &File, extents: &[Extent], offset: u64, buf: &mut [u8]) -> io::Result<()> {
    let mut done = 0;
    while done < buf.len() {
        let at = offset + done as u64;
        let ix = extents.partition_point(|e| e.start + e.len <= at);
        let extent = extents.get(ix).filter(|e| e.start <= at).ok_or(io::ErrorKind::InvalidData)?;
        let take = ((extent.start + extent.len - at) as usize).min(buf.len() - done);
        read_at(vol, &mut buf[done..done + take], extent.disk + (at - extent.start))?;
        done += take;
    }
    Ok(())
}

/// Decodes an attribute's data runs: (length, signed offset from the previous run)
/// pairs of variable-width little-endian numbers.
fn parse_runs(mut runs: &[u8], cluster: u64) -> Option<Vec<Extent>> {
    let mut extents = Vec::new();
    let (mut lcn, mut start) = (0i64, 0u64);
    while let Some((&header, rest)) = runs.split_first() {
        if header == 0 {
            break;
        }
        let (len_size, off_size) = ((header & 0xF) as usize, (header >> 4) as usize);
        // The file table is never sparse, so a run must say where it is.
        if len_size == 0 || len_size > 8 || off_size == 0 || off_size > 8 || rest.len() < len_size + off_size {
            return None;
        }
        let mut len = [0u8; 8];
        len[..len_size].copy_from_slice(&rest[..len_size]);
        let mut off = [if rest[len_size + off_size - 1] & 0x80 != 0 { 0xFF } else { 0 }; 8];
        off[..off_size].copy_from_slice(&rest[len_size..len_size + off_size]);
        lcn = lcn.checked_add(i64::from_le_bytes(off))?;
        let bytes = u64::from_le_bytes(len).checked_mul(cluster)?;
        extents.push(Extent { start, disk: u64::try_from(lcn).ok()?.checked_mul(cluster)?, len: bytes });
        start += bytes;
        runs = &rest[len_size + off_size..];
    }
    Some(extents)
}

/// Undoes NTFS's torn-write protection: the last two bytes of each 512-byte sector
/// hold a check value, with the real bytes kept in the record's header. A record that
/// fails the check is unusable.
fn fix_up(rec: &mut [u8]) -> bool {
    if &rec[..4] != b"FILE" {
        return false;
    }
    let (array, count) = (usize::from(u16at(rec, 4)), usize::from(u16at(rec, 6)));
    if count < 2 || array + count * 2 > rec.len() {
        return false;
    }
    let check = [rec[array], rec[array + 1]];
    for i in 1..count {
        let end = i * 512;
        if end > rec.len() || rec[end - 2..end] != check {
            return false;
        }
        rec.copy_within(array + i * 2..array + i * 2 + 2, end - 2);
    }
    true
}

struct Attr<'a> {
    kind: u32,
    bytes: &'a [u8],
}

impl Attr<'_> {
    fn non_resident(&self) -> bool {
        self.bytes[8] != 0
    }

    fn unnamed(&self) -> bool {
        self.bytes[9] == 0
    }

    fn value(&self) -> Option<&[u8]> {
        if self.non_resident() {
            return None;
        }
        let (len, at) = (u32at(self.bytes, 0x10) as usize, usize::from(u16at(self.bytes, 0x14)));
        self.bytes.get(at..at.checked_add(len)?)
    }

    fn is_wof_stream(&self) -> bool {
        let (at, len) = (usize::from(u16at(self.bytes, 0x0A)), usize::from(self.bytes[9]));
        self.bytes.get(at..at + len * 2).is_some_and(|name| {
            name.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).eq(WOF_STREAM.encode_utf16())
        })
    }

    /// What a stream occupies on disk.
    fn allocated(&self) -> Option<u64> {
        if !self.non_resident() {
            return Some(u64::from(u32at(self.bytes, 0x10)));
        }
        (u64at(self.bytes, 0x10) == 0).then(|| u64at(self.bytes, 0x28))
    }

    /// A data attribute's length, and what it occupies if it is compressed or sparse.
    fn data_size(&self) -> Option<(u64, u64)> {
        if !self.non_resident() {
            return Some((u64::from(u32at(self.bytes, 0x10)), NO_PACKED));
        }
        if u64at(self.bytes, 0x10) != 0 || self.bytes.len() < 0x40 {
            return None; // a later piece of a fragmented attribute
        }
        let packed = if u16at(self.bytes, 0x0C) & (ATTR_COMPRESSED | ATTR_SPARSE) == 0 {
            NO_PACKED
        } else if u16at(self.bytes, 0x22) != 0 && self.bytes.len() >= 0x48 {
            u64at(self.bytes, 0x40)
        } else {
            u64at(self.bytes, 0x28)
        };
        Some((u64at(self.bytes, 0x30), packed))
    }
}

fn attributes(rec: &[u8]) -> impl Iterator<Item = Attr<'_>> {
    let used = (u32at(rec, 0x18) as usize).min(rec.len());
    let mut at = usize::from(u16at(rec, 0x14));
    std::iter::from_fn(move || {
        if at + 16 > used || u32at(rec, at) == END {
            return None;
        }
        let len = u32at(rec, at + 4) as usize;
        if len < 16 || at + len > used {
            return None;
        }
        let attr = Attr { kind: u32at(rec, at), bytes: &rec[at..at + len] };
        at += len;
        Some(attr)
    })
}

/// A name of a record in a folder.
struct Name {
    parent: u32,
    record: u32,
    off: u32,
    len: u32,
}

/// Data attributes found in extension records: pieces of a file whose attributes
/// overflowed its own record. `wof` marks the stream holding system-compressed data,
/// whose `real` is what it occupies.
struct Extra {
    base: u32,
    real: u64,
    packed: u64,
    wof: bool,
}

#[derive(Default)]
struct Part {
    names: Vec<Name>,
    arena: String,
    extra: Vec<Extra>,
}

impl Part {
    fn add_name(&mut self, value: &[u8], record: u32) {
        let len = usize::from(*value.get(0x40).unwrap_or(&0));
        let Some(chars) = value.get(0x42..0x42 + len * 2) else { return };
        let Ok(parent) = u32::try_from(u64at(value, 0) & REF_MASK) else { return };
        // The short 8.3 name of a file is a second name for the same link.
        if len == 0 || value[0x41] == NAMESPACE_DOS {
            return;
        }
        let off = self.arena.len();
        let units = chars.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]));
        self.arena.extend(char::decode_utf16(units).map(|c| c.unwrap_or('\u{FFFD}')));
        self.names.push(Name { parent, record, off: off as u32, len: (self.arena.len() - off) as u32 });
    }
}

fn pick(flags: u8, real: u64, packed: u64) -> u64 {
    if flags & SPECIAL != 0 && packed != NO_PACKED { packed } else { real }
}

/// Reads one record of the file table into its slot of `sizes` and `flags`, and its
/// names into `part`.
fn parse_record(rec: &mut [u8], number: u32, size: &mut u64, flags: &mut u8, part: &mut Part) {
    if !fix_up(rec) {
        return;
    }
    let header = u16at(rec, 0x16);
    let Ok(base) = u32::try_from(u64at(rec, 0x20) & REF_MASK) else { return };
    if header & 1 == 0 {
        return;
    }
    let owner = if base == 0 { number } else { base };
    let (mut attributes_of_file, mut tag, mut name_tag) = (0u32, 0u32, 0u32);
    let (mut data, mut wof) = (None, None);
    for attr in attributes(rec) {
        match attr.kind {
            STANDARD_INFORMATION => {
                attributes_of_file = attr.value().map_or(0, |v| u32at(v, 0x20));
            }
            FILE_NAME => {
                if let Some(value) = attr.value() {
                    part.add_name(value, owner);
                    // A file with many names pushes its reparse attribute into another
                    // record, but each name keeps a copy of the tag.
                    if u32at(value, 0x38) & ATTR_REPARSE != 0 {
                        name_tag = u32at(value, 0x3C);
                    }
                }
            }
            DATA if attr.unnamed() => data = data.or(attr.data_size()),
            DATA if attr.is_wof_stream() => wof = wof.or(attr.allocated()),
            REPARSE_POINT => tag = attr.value().map_or(0, |v| u32at(v, 0)),
            _ => {}
        }
    }
    if base != 0 {
        if let Some((real, packed)) = data {
            part.extra.push(Extra { base, real, packed, wof: false });
        }
        if let Some(real) = wof {
            part.extra.push(Extra { base, real, packed: NO_PACKED, wof: true });
        }
        return;
    }
    let tag = if tag != 0 { tag } else { name_tag };
    let mut f = IN_USE;
    let link = tag == REPARSE_SYMLINK || tag == REPARSE_MOUNT_POINT;
    if header & 2 != 0 && !link {
        f |= DIR;
    }
    if attributes_of_file & SPECIAL_ATTRIBUTES != 0 {
        f |= SPECIAL;
    }
    if tag == REPARSE_WOF {
        f |= WOF;
    }
    if let Some((real, packed)) = data {
        f |= HAS_DATA;
        *size = pick(f, real, packed);
    }
    if f & (WOF | SPECIAL) == WOF | SPECIAL
        && let Some(stored) = wof
    {
        f |= HAS_DATA;
        *size = stored;
    }
    *flags = f;
}

fn drive_letter(root: &Path) -> Option<char> {
    match root.to_str()?.as_bytes() {
        [letter, b':', b'\\' | b'/'] if letter.is_ascii_alphabetic() => Some(*letter as char),
        _ => None,
    }
}

pub fn scan(root: &Path, display_name: &str, progress: &Progress) -> Result<Tree, Failure> {
    let started = Instant::now();
    let letter = drive_letter(root).ok_or(Failure::Unsupported)?;
    let device = format!(r"\\.\{letter}:");
    let vol = File::open(&device).map_err(|_| Failure::Unsupported)?;
    let layout = read_layout(&vol).ok_or(Failure::Unsupported)?;
    let Layout { cluster, record, mft_lcn } = layout;
    if let Ok(mut current) = progress.current.lock() {
        *current = "Reading the file table".to_string();
    }

    // The table describes itself in its first record.
    let mut first = vec![0u8; record];
    read_at(&vol, &mut first, mft_lcn * cluster).map_err(|_| Failure::Unsupported)?;
    if !fix_up(&mut first) {
        return Err(Failure::Unsupported);
    }
    let (extents, table_len) = attributes(&first)
        .find(|a| a.kind == DATA && a.unnamed() && a.non_resident() && u64at(a.bytes, 0x10) == 0)
        .and_then(|a| {
            let at = usize::from(u16at(a.bytes, 0x20));
            Some((parse_runs(a.bytes.get(at..)?, cluster)?, u64at(a.bytes, 0x30)))
        })
        .ok_or(Failure::Unsupported)?;
    // A table fragmented enough to need extension records for its own runs isn't handled.
    let mapped: u64 = extents.iter().map(|e| e.len).sum();
    let records = usize::try_from(table_len / record as u64).map_err(|_| Failure::Unsupported)?;
    if mapped < table_len || records <= FIRST_USER_RECORD as usize || u32::try_from(records).is_err() {
        return Err(Failure::Unsupported);
    }

    // One thread reads the table front to back, because the drive is fastest that way
    // and slows to a crawl when several readers interleave; a second parses what it
    // has read while the next chunk comes in.
    const BUFFERS: usize = 6;
    let (full_tx, full_rx) = std::sync::mpsc::sync_channel::<(usize, Aligned)>(BUFFERS);
    let (free_tx, free_rx) = std::sync::mpsc::channel::<Aligned>();
    for _ in 0..BUFFERS {
        let _ = free_tx.send(Aligned::new(CHUNK * record));
    }
    let reader = open_unbuffered(&device).ok_or(Failure::Unsupported)?;
    let (mut sizes, mut flags, Part { names, arena, extra }) = std::thread::scope(|scope| {
        let parser = scope.spawn(move || {
            let mut sizes = vec![0u64; records];
            let mut flags = vec![0u8; records];
            let mut part = Part::default();
            for (chunk, mut buf) in full_rx {
                let first = chunk * CHUNK;
                let count = (records - first).min(CHUNK);
                for (i, rec) in buf.bytes()[..count * record].chunks_exact_mut(record).enumerate() {
                    let n = first + i;
                    parse_record(rec, n as u32, &mut sizes[n], &mut flags[n], &mut part);
                }
                let files = flags[first..first + count].iter().filter(|f| **f & (IN_USE | DIR) == IN_USE).count();
                progress.files.fetch_add(files as u64, Ordering::Relaxed);
                progress.bytes.fetch_add(sizes[first..first + count].iter().sum(), Ordering::Relaxed);
                let _ = free_tx.send(buf);
            }
            (sizes, flags, part)
        });
        let mut outcome = Ok(());
        for chunk in 0..records.div_ceil(CHUNK) {
            if progress.cancelled.load(Ordering::Relaxed) {
                outcome = Err(Failure::Cancelled);
                break;
            }
            let Ok(mut buf) = free_rx.recv() else { break };
            let len = (records - chunk * CHUNK).min(CHUNK) * record;
            let offset = (chunk * CHUNK * record) as u64;
            if read_table(&reader, &extents, offset, &mut buf.bytes()[..len]).is_err() {
                outcome = Err(Failure::Unsupported);
                break;
            }
            if full_tx.send((chunk, buf)).is_err() {
                break;
            }
        }
        drop(full_tx);
        let parsed = parser.join().map_err(|_| Failure::Unsupported)?;
        outcome.map(|()| parsed)
    })?;
    if let Ok(mut current) = progress.current.lock() {
        *current = "Building the tree".to_string();
    }

    for Extra { base, real, packed, wof } in extra {
        let base = base as usize;
        if base >= records || flags[base] & IN_USE == 0 {
            continue;
        }
        if wof {
            if flags[base] & (WOF | SPECIAL) == WOF | SPECIAL {
                flags[base] |= HAS_DATA;
                sizes[base] = real;
            }
        } else if flags[base] & HAS_DATA == 0 {
            flags[base] |= HAS_DATA;
            sizes[base] = pick(flags[base], real, packed);
        }
    }
    if flags[ROOT_RECORD as usize] & DIR == 0 {
        return Err(Failure::Unsupported);
    }

    // Group the names by folder, skipping those of bookkeeping files and of records
    // that aren't in use.
    let kept = |n: &Name| {
        n.record >= FIRST_USER_RECORD
            && (n.parent as usize) < records
            && n.parent != n.record
            && flags[n.record as usize] & IN_USE != 0
    };
    let mut start = vec![0u32; records + 1];
    for n in names.iter().filter(|n| kept(n)) {
        start[n.parent as usize + 1] += 1;
    }
    for i in 0..records {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut order = vec![0u32; start[records] as usize];
    for (i, n) in names.iter().enumerate().filter(|(_, n)| kept(n)) {
        let slot = &mut fill[n.parent as usize];
        order[*slot as usize] = i as u32;
        *slot += 1;
    }

    let table = Table { names: &names, arena: &arena, order: &order, start: &start, sizes: &sizes, flags: &flags };
    let mut entered = vec![false; records];
    entered[ROOT_RECORD as usize] = true;
    let dir = table.build(ROOT_RECORD as usize, &mut entered);
    if dir.items == 0 {
        return Err(Failure::Unsupported);
    }
    let mut tree = flatten(dir, display_name, 0, root.to_path_buf());
    tree.duration = started.elapsed();
    Ok(tree)
}

struct Table<'a> {
    names: &'a [Name],
    arena: &'a str,
    order: &'a [u32],
    start: &'a [u32],
    sizes: &'a [u64],
    flags: &'a [u8],
}

impl Table<'_> {
    fn build(&self, folder: usize, entered: &mut [bool]) -> Dir {
        let mut out = Dir::default();
        for &ix in &self.order[self.start[folder] as usize..self.start[folder + 1] as usize] {
            let name = &self.names[ix as usize];
            let record = name.record as usize;
            let is_dir = self.flags[record] & DIR != 0;
            // A folder reached twice means a damaged table; keep it where it was first met.
            if is_dir && std::mem::replace(&mut entered[record], true) {
                continue;
            }
            out.names.push_str(&self.arena[name.off as usize..(name.off + name.len) as usize]);
            let (size, dir) = if is_dir {
                let sub = self.build(record, entered);
                (sub.size, Some(Box::new(sub)))
            } else {
                (self.sizes[record], None)
            };
            out.children.push(Child { name_end: out.names.len() as u32, size, dir });
        }
        out.size = out.children.iter().map(|c| c.size).sum();
        out.items = out.children.iter().map(Child::items).sum();
        out.names.shrink_to_fit();
        out.children.shrink_to_fit();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_data_runs() {
        // 0x20 clusters at LCN 0x100, then 0x10 more 0x40 clusters back.
        let runs = [0x21, 0x20, 0x00, 0x01, 0x21, 0x10, 0xC0, 0xFF, 0x00];
        let extents = parse_runs(&runs, 4096).unwrap();
        assert_eq!(extents.len(), 2);
        assert_eq!((extents[0].start, extents[0].disk, extents[0].len), (0, 0x100 * 4096, 0x20 * 4096));
        assert_eq!((extents[1].start, extents[1].disk, extents[1].len), (0x20 * 4096, 0xC0 * 4096, 0x10 * 4096));
        assert!(parse_runs(&[0x01, 0x10, 0x00, 0x00], 4096).is_none()); // sparse run
    }

    #[test]
    fn restores_sector_ends() {
        let mut rec = vec![0u8; 1024];
        rec[..4].copy_from_slice(b"FILE");
        rec[4..6].copy_from_slice(&48u16.to_le_bytes());
        rec[6..8].copy_from_slice(&3u16.to_le_bytes());
        rec[48..50].copy_from_slice(&[0xAB, 0xCD]); // check value
        rec[50..52].copy_from_slice(&[1, 2]);
        rec[52..54].copy_from_slice(&[3, 4]);
        rec[510..512].copy_from_slice(&[0xAB, 0xCD]);
        rec[1022..1024].copy_from_slice(&[0xAB, 0xCD]);
        assert!(fix_up(&mut rec));
        assert_eq!(&rec[510..512], &[1, 2]);
        assert_eq!(&rec[1022..1024], &[3, 4]);
        rec[510] = 0;
        assert!(!fix_up(&mut rec));
    }

    #[test]
    fn recognises_drive_roots() {
        assert_eq!(drive_letter(Path::new(r"C:\")), Some('C'));
        assert_eq!(drive_letter(Path::new(r"d:\")), Some('d'));
        assert_eq!(drive_letter(Path::new(r"C:\Users")), None);
        assert_eq!(drive_letter(Path::new("C:")), None);
    }

    /// Needs an elevated shell: compares the file-table scan with the walk of C:. The two
    /// differ by what the walk can't read (System Volume Information), by files that
    /// changed in between, and by Windows-compressed files, which the walk can only
    /// count at their uncompressed length.
    #[test]
    #[ignore]
    fn matches_the_walk() {
        let root = Path::new(r"C:\");
        let timed = |f: &dyn Fn() -> Option<Tree>| {
            let t = Instant::now();
            let tree = f().expect("scan failed");
            (t.elapsed(), tree)
        };
        let (fast, a) = timed(&|| scan(root, "C", &Progress::default()).ok());
        let (slow, b) = timed(&|| super::super::scan(root, "C", &Progress::default()));
        let (na, nb) = (a.node(Tree::ROOT), b.node(Tree::ROOT));
        eprintln!("mft  {fast:?}: {} bytes, {} files", na.size, na.items);
        eprintln!("walk {slow:?}: {} bytes, {} files", nb.size, nb.items);
        let unreadable = na
            .children
            .iter()
            .map(|c| a.node(*c))
            .find(|n| n.name == "System Volume Information")
            .map_or(0, |n| n.size);
        let (x, y) = ((na.size - unreadable) as f64, nb.size as f64);
        assert!((x - y).abs() / y < 0.01, "sizes differ by more than 1%");
        assert!((na.items as f64 - nb.items as f64).abs() / (nb.items as f64) < 0.001, "file counts differ");
    }
}
