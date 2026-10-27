#!/bin/sh
# Builds Burrdisk as "Burrdisk Dev.app", signed with a stable local certificate.
#
# macOS ties Full Disk Access to an app's code signature. A plain `cargo run`
# binary is re-signed ad hoc on every build, so the grant never sticks and
# protected folders prompt again. Signing every build with the same certificate
# keeps one identity, so Full Disk Access is granted once.
#
#   script/dev-app.sh            build (debug), sign, launch
#   script/dev-app.sh --release  same with the release profile
#   script/dev-app.sh --grant    also open the Full Disk Access settings pane
#   script/dev-app.sh -- ~/Library   pass arguments to the app
set -eu

IDENTITY="Burrdisk Dev"
BUNDLE_ID="dev.burrdisk.Burrdisk"
ROOT=$(cd "$(dirname "$0")/.." && pwd)
PROFILE=debug
CARGO_FLAGS=""
GRANT=0
while [ $# -gt 0 ]; do
    case "$1" in
        --release) PROFILE=release; CARGO_FLAGS="--release" ;;
        --grant) GRANT=1 ;;
        --) shift; break ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

# One-time: a self-signed code signing certificate in the login keychain.
if ! security find-certificate -c "$IDENTITY" >/dev/null 2>&1; then
    echo "Creating the \"$IDENTITY\" code signing certificate (one time)…"
    TMP=$(mktemp -d)
    trap 'rm -rf "$TMP"' EXIT
    cat > "$TMP/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $IDENTITY
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
    openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
        -keyout "$TMP/key.pem" -out "$TMP/cert.pem" -config "$TMP/cert.cnf" 2>/dev/null
    openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
        -out "$TMP/id.p12" -passout pass:burrdisk -name "$IDENTITY" \
        -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 2>/dev/null
    security import "$TMP/id.p12" -k "$HOME/Library/Keychains/login.keychain-db" \
        -P burrdisk -T /usr/bin/codesign >/dev/null
fi

cd "$ROOT"
cargo build $CARGO_FLAGS

APP="$ROOT/target/$PROFILE/Burrdisk Dev.app"
mkdir -p "$APP/Contents/MacOS"
cp "target/$PROFILE/burrdisk" "$APP/Contents/MacOS/burrdisk"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>burrdisk</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleName</key><string>Burrdisk Dev</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --sign "$IDENTITY" --identifier "$BUNDLE_ID" "$APP" 2>&1 | grep -v "replacing existing signature" || true

if [ "$GRANT" = 1 ]; then
    echo "Add \"$APP\" to Full Disk Access (drag it in, or use +). This is needed once."
    open -R "$APP"
    open "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
fi

# Quit a running copy so the new build takes over.
pkill -f "$APP/Contents/MacOS/burrdisk" 2>/dev/null || true
open -n "$APP" --args "$@"
