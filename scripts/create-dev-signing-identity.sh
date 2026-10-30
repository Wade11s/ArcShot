#!/bin/sh
# Creates the local "ArcShot Dev" code-signing identity used by build-app.sh.
#
# A stable signing identity keeps the app's designated requirement (and thus
# its TCC Screen Recording grant) valid across rebuilds. Ad-hoc signatures are
# cdhash-bound, so every rebuild invalidates the grant. This identity is for
# local testing only: it is self-signed, never trusted by other machines, and
# not part of release signing or notarization.
set -eu

cd "$(dirname "$0")/.."

identity='ArcShot Dev'
keychain="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning 2>/dev/null | grep -q "\"$identity\""; then
    printf 'signing_identity=exists name=%s\n' "$identity"
    exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
    -keyout "$work/key.pem" -out "$work/cert.pem" \
    -subj "/CN=$identity" \
    -addext "basicConstraints=critical,CA:false" \
    -addext "keyUsage=critical,digitalSignature" \
    -addext "extendedKeyUsage=critical,codeSigning" >/dev/null 2>&1

# macOS Security cannot verify OpenSSL 3+/4 default PKCS#12 MACs; the legacy
# SHA-1/3DES form imports reliably. LibreSSL (macOS's own openssl) rejects
# -legacy, so fall back to its default legacy output.
if ! openssl pkcs12 -export -legacy -macalg sha1 \
    -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES \
    -inkey "$work/key.pem" -in "$work/cert.pem" -out "$work/identity.p12" \
    -passout pass:arcshot -name "$identity" >/dev/null 2>&1; then
    openssl pkcs12 -export -macalg sha1 \
        -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES \
        -inkey "$work/key.pem" -in "$work/cert.pem" -out "$work/identity.p12" \
        -passout pass:arcshot -name "$identity" >/dev/null 2>&1
fi

security import "$work/identity.p12" -k "$keychain" -P arcshot \
    -T /usr/bin/codesign -T /usr/bin/security >/dev/null

# Trust the leaf for code signing in the user domain so find-identity lists it
# as valid; other machines still reject it.
security add-trusted-cert -r trustRoot -p codeSign -k "$keychain" "$work/cert.pem"

if ! security find-identity -v -p codesigning 2>/dev/null | grep -q "\"$identity\""; then
    printf 'signing identity could not be created\n' >&2
    exit 1
fi
printf 'signing_identity=created name=%s\n' "$identity"
