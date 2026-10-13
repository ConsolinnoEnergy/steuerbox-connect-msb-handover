#!/usr/bin/env bash
# test/ca — generate the local test PKI (output gitignored, never committed).
#
# Domains (AGENTS.md §4):
#   1. Server-TLS (sim-LE): root -> "sim-LE intermediate" -> server leafs (msb, wellknown)
#   2. Client-mTLS: root -> dedicated client sub-CA (v1 + v2 for rotation) -> client leafs
#   3. Rogue: self-contained CA + client cert (for RogueClientCA e2e)
set -euo pipefail
umask 077
cd "$(dirname "$0")"
OUT=${OUT:-out}
mkdir -p "$OUT"
cd "$OUT"

DAYS_ROOT=3650
DAYS_SUB=1825
DAYS_LEAF=1   # sim-LE: 24 h leaf validity, LE-like

# --- helpers -------------------------------------------------------------
key() { openssl ecparam -name prime256v1 -genkey -noout -out "$1"; }
csr() { openssl req -new -key "$1" -subj "$2" -out "$3"; }
sign() { # sign <csr> <extfile> <out-cert> <ca-key> <ca-cert> <days>
  openssl x509 -req -in "$1" -CA "$5" -CAkey "$4" -CAcreateserial \
    -extfile "$2" -days "$6" -out "$3" 2>/dev/null
}

# --- extensions ----------------------------------------------------------
cat > root.ext <<'EOF'
basicConstraints=critical,CA:true
keyUsage=critical,keyCertSign,cRLSign
subjectKeyIdentifier=hash
authorityKeyIdentifier=keyid:always
EOF
cat > sub.ext <<'EOF'
basicConstraints=critical,CA:true,pathlen:0
keyUsage=critical,keyCertSign,cRLSign
subjectKeyIdentifier=hash
authorityKeyIdentifier=keyid:always
EOF
server_ext() { cat > "$1" <<EOF
basicConstraints=CA:false
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=$2
subjectKeyIdentifier=hash
authorityKeyIdentifier=keyid:always
EOF
}
client_ext() { cat > "$1" <<EOF
basicConstraints=CA:false
keyUsage=critical,digitalSignature
extendedKeyUsage=clientAuth
subjectKeyIdentifier=hash
authorityKeyIdentifier=keyid:always
EOF
}

# --- 1. sim-LE root ------------------------------------------------------
[ -f root-cert.pem ] || {
  key root-key.pem
  csr root-key.pem "/CN=steuerbox-connect sim-LE Root/O=Test CA/C=DE" root.csr
  openssl x509 -req -in root.csr -signkey root-key.pem -extfile root.ext \
    -days "$DAYS_ROOT" -out root-cert.pem 2>/dev/null
}

# --- 2. sim-LE intermediate + server leafs -------------------------------
if [ ! -f server-int-cert.pem ]; then
  key server-int-key.pem
  csr server-int-key.pem "/CN=steuerbox-connect sim-LE Intermediate/O=Test CA/C=DE" server-int.csr
  sign server-int.csr sub.ext server-int-cert.pem root-key.pem root-cert.pem "$DAYS_SUB"
fi
server_ext msb.ext "DNS:msb.local,DNS:localhost,IP:127.0.0.1"
[ -f msb-cert.pem ] || {
  key msb-key.pem
  csr msb-key.pem "/CN=msb.local/O=Reference MSB/C=DE" msb.csr
  sign msb.csr msb.ext msb-cert.pem server-int-key.pem server-int-cert.pem "$DAYS_LEAF"
}
server_ext wellknown.ext "DNS:wellknown.local,DNS:localhost,IP:127.0.0.1"
[ -f wellknown-key.pem ] || {
  key wellknown-key.pem
  csr wellknown-key.pem "/CN=wellknown.local/O=EMS Manufacturer/C=DE" wellknown.csr
  sign wellknown.csr wellknown.ext wellknown-cert.pem server-int-key.pem server-int-cert.pem "$DAYS_LEAF"
}
# leaf + intermediate chain (served to TLS peers)
cat msb-cert.pem server-int-cert.pem > msb-chain.pem
cat wellknown-cert.pem server-int-cert.pem > wellknown-chain.pem

# --- 3. dedicated client sub-CA (v1) + client leaf -----------------------
if [ ! -f client-root-cert.pem ]; then
  key client-root-key.pem
  csr client-root-key.pem "/CN=ExampleEMS Client Root/O=Test CA/C=DE" client-root.csr
  openssl x509 -req -in client-root.csr -signkey client-root-key.pem -extfile root.ext \
    -days "$DAYS_ROOT" -out client-root-cert.pem 2>/dev/null
fi
if [ ! -f client-ca-cert.pem ]; then
  key client-ca-key.pem
  csr client-ca-key.pem "/CN=ExampleEMS steuerbox-connect Client CA/O=ExampleEMS/C=DE" client-ca.csr
  sign client-ca.csr sub.ext client-ca-cert.pem client-root-key.pem client-root-cert.pem "$DAYS_SUB"
fi
client_ext client.ext
[ -f client-cert.pem ] || {
  key client-key.pem
  csr client-key.pem "/CN=ems-backend-1/O=ExampleEMS/C=DE" client.csr
  sign client.csr client.ext client-cert.pem client-ca-key.pem client-ca-cert.pem "$DAYS_LEAF"
}

# --- 4. client sub-CA v2 (for ClientCARotation) + rotated leaf ----------
if [ ! -f client-ca-v2-cert.pem ]; then
  key client-ca-v2-key.pem
  csr client-ca-v2-key.pem "/CN=ExampleEMS steuerbox-connect Client CA v2/O=ExampleEMS/C=DE" client-ca-v2.csr
  sign client-ca-v2.csr sub.ext client-ca-v2-cert.pem client-root-key.pem client-root-cert.pem "$DAYS_SUB"
fi
[ -f client-v2-cert.pem ] || {
  key client-v2-key.pem
  csr client-v2-key.pem "/CN=ems-backend-2/O=ExampleEMS/C=DE" client-v2.csr
  sign client-v2.csr client.ext client-v2-cert.pem client-ca-v2-key.pem client-ca-v2-cert.pem "$DAYS_LEAF"
}
# second leaf under the ORIGINAL client CA (ClientCertRotation)
[ -f client-b-cert.pem ] || {
  key client-b-key.pem
  csr client-b-key.pem "/CN=ems-backend-1-rotated/O=ExampleEMS/C=DE" client-b.csr
  sign client-b.csr client.ext client-b-cert.pem client-ca-key.pem client-ca-cert.pem "$DAYS_LEAF"
}

# --- 5. rogue CA + rogue client cert (RogueClientCA e2e) -----------------
if [ ! -f rogue-ca-cert.pem ]; then
  key rogue-ca-key.pem
  csr rogue-ca-key.pem "/CN=Rogue Client CA/O=Attacker/C=DE" rogue-ca.csr
  openssl x509 -req -in rogue-ca.csr -signkey rogue-ca-key.pem -extfile root.ext \
    -days "$DAYS_ROOT" -out rogue-ca-cert.pem 2>/dev/null
fi
[ -f rogue-client-cert.pem ] || {
  key rogue-client-key.pem
  csr rogue-client-key.pem "/CN=rogue-client/O=Attacker/C=DE" rogue-client.csr
  sign rogue-client.csr client.ext rogue-client-cert.pem rogue-ca-key.pem rogue-ca-cert.pem "$DAYS_LEAF"
}

# --- 6. EMS QR keypair (hems_eebus_local_qr encryption, audit-tested) -------------
if [ ! -f ems-qr-key.pem ]; then
  openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:4096 -out ems-qr-key.pem 2>/dev/null
  openssl pkey -in ems-qr-key.pem -pubout -out ems-qr-pub.pem 2>/dev/null
fi

# --- bundles -------------------------------------------------------------
# clients trust the sim-LE root for server TLS
cat root-cert.pem > server-roots.pem
# msb pin store: current client CA only (rotation tests add v2 with grace)
mkdir -p pins pins-rogue
mkdir -p backend-pins
# Writable mount for the unprivileged one-shot backend; no keys live here.
chmod 777 backend-pins
cat client-ca-cert.pem > pins/client-ca.pem
cat rogue-ca-cert.pem > pins-rogue/rogue-ca.pem

echo "test CA ready in $PWD"
