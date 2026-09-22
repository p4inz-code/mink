"""Generate the TLS test PKI (run once; the PEMs are committed).

Usage: python3 tests/tls/make_certs.py tests/tls

Every certificate has fixed validity dates, so the fixtures are reproducible
and the "expired" one is expired for good. The suite's server uses
`<name>.pem` + `<name>.key.pem`; the client trusts `ca.pem`.

  ca.pem                self-signed test CA ("MINK Test CA")
  good.pem              SAN localhost, serverAuth, issued by the CA
  wrongname.pem         SAN other.example, serverAuth
  sanmulti.pem          SAN other.example + localhost (match on the 2nd entry)
  clientauth.pem        SAN localhost, clientAuth only (wrong usage)
  expired.pem           SAN localhost, serverAuth, not valid after 2021
  wildcard.pem          SAN *.localhost (a wildcard needs a second label)
  ipliteral.pem         SAN iPAddress 127.0.0.1
  untrusted.pem         self-signed for localhost (not issued by the CA)
"""

import datetime
import os
import sys

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

OUT = sys.argv[1] if len(sys.argv) > 1 else "."
os.makedirs(OUT, exist_ok=True)
VALID_FROM = datetime.datetime(2020, 1, 1, tzinfo=datetime.timezone.utc)
VALID_TO = datetime.datetime(2035, 1, 1, tzinfo=datetime.timezone.utc)
EXPIRED_FROM = datetime.datetime(2020, 1, 1, tzinfo=datetime.timezone.utc)
EXPIRED_TO = datetime.datetime(2021, 1, 1, tzinfo=datetime.timezone.utc)


def write(name, data):
    with open(os.path.join(OUT, name), "wb") as fh:
        fh.write(data)


def pem_cert(cert):
    return cert.public_bytes(serialization.Encoding.PEM)


def pem_key(key):
    return key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.TraditionalOpenSSL,
        serialization.NoEncryption(),
    )


def new_key(seed):
    # A fixed exponent/key size keeps the fixtures small and reproducible in
    # shape (the serial numbers below are fixed too).
    return rsa.generate_private_key(public_exponent=65537, key_size=2048)


ca_key = new_key(1)
ca_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "MINK Test CA")])
ca = (
    x509.CertificateBuilder()
    .subject_name(ca_name)
    .issuer_name(ca_name)
    .public_key(ca_key.public_key())
    .serial_number(0x4D1)
    .not_valid_before(VALID_FROM)
    .not_valid_after(VALID_TO)
    .add_extension(x509.BasicConstraints(ca=True, path_length=1), critical=True)
    .add_extension(
        x509.KeyUsage(
            digital_signature=True,
            content_commitment=False,
            key_encipherment=False,
            data_encipherment=False,
            key_agreement=False,
            key_cert_sign=True,
            crl_sign=True,
            encipher_only=False,
            decipher_only=False,
        ),
        critical=True,
    )
    .sign(ca_key, hashes.SHA256())
)
write("ca.pem", pem_cert(ca))


def leaf(name, sans, eku, serial, not_before=VALID_FROM, not_after=VALID_TO, issuer=ca_name, issuer_key=ca_key, subject=None):
    key = new_key(serial)
    subject_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, subject or name)])
    builder = (
        x509.CertificateBuilder()
        .subject_name(subject_name)
        .issuer_name(issuer)
        .public_key(key.public_key())
        .serial_number(serial)
        .not_valid_before(not_before)
        .not_valid_after(not_after)
        .add_extension(x509.SubjectAlternativeName(sans), critical=False)
    )
    if eku is not None:
        builder = builder.add_extension(x509.ExtendedKeyUsage([eku]), critical=False)
    return key, builder.sign(issuer_key, hashes.SHA256())


def emit(name, key, cert):
    write(name + ".pem", pem_cert(cert))
    write(name + ".key.pem", pem_key(key))


import ipaddress  # noqa: E402

DNS = x509.DNSName
SERVER = ExtendedKeyUsageOID.SERVER_AUTH
CLIENT = ExtendedKeyUsageOID.CLIENT_AUTH

for name, sans, eku, serial, nb, na in [
    ("good", [DNS("localhost")], SERVER, 0x1001, VALID_FROM, VALID_TO),
    ("wrongname", [DNS("other.example")], SERVER, 0x1002, VALID_FROM, VALID_TO),
    ("sanmulti", [DNS("other.example"), DNS("localhost")], SERVER, 0x1003, VALID_FROM, VALID_TO),
    ("clientauth", [DNS("localhost")], CLIENT, 0x1004, VALID_FROM, VALID_TO),
    ("expired", [DNS("localhost")], SERVER, 0x1005, EXPIRED_FROM, EXPIRED_TO),
    ("wildcard", [DNS("*.localhost")], SERVER, 0x1006, VALID_FROM, VALID_TO),
    ("ipliteral", [x509.IPAddress(ipaddress.IPv4Address("127.0.0.1"))], SERVER, 0x1007, VALID_FROM, VALID_TO),
]:
    key, cert = leaf(name, sans, eku, serial, nb, na)
    emit(name, key, cert)

self_key = new_key(0x2000)
self_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "localhost")])
self_cert = (
    x509.CertificateBuilder()
    .subject_name(self_name)
    .issuer_name(self_name)
    .public_key(self_key.public_key())
    .serial_number(0x2000)
    .not_valid_before(VALID_FROM)
    .not_valid_after(VALID_TO)
    .add_extension(x509.SubjectAlternativeName([DNS("localhost")]), critical=False)
    .add_extension(x509.ExtendedKeyUsage([SERVER]), critical=False)
    .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
    .sign(self_key, hashes.SHA256())
)
emit("untrusted", self_key, self_cert)
print("wrote TLS fixtures to", OUT)
