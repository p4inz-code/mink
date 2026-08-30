# MINK Security Architecture

**Version:** 0.1.0 (Design Draft)
**Date:** August 25, 2026
**Status:** DESIGN ONLY — not implemented
**Scope:** Security and supply chain protection for the MINK package ecosystem

---

## 1. Goals

1. **Proactive security.** Security is designed BEFORE the package ecosystem grows, not as an afterthought.

2. **Package integrity.** Every package is verified by content hash before use.

3. **Supply chain protection.** Malicious packages, dependency confusion, and typosquatting are detected and prevented.

4. **Build safety.** Build scripts run in sandboxed environments with restricted permissions.

5. **Transparency.** All security-relevant operations are logged and auditable.

---

## 2. Package Integrity

### Content hashing
- Every package version has a SHA-256 content hash
- The hash covers the entire package archive (source code, metadata, build scripts)
- Hashes are stored in the registry and the lockfile

### Verification process
1. Download package archive
2. Compute SHA-256 hash of the archive
3. Compare against the hash in the lockfile
4. If mismatch → reject the package, report error
5. If match → extract and use

### Hash format

```
sha256:abc123def456...
```

### Lockfile integrity
- The lockfile records hashes for all dependencies
- The lockfile itself is committed to version control
- Tampering with the lockfile is detected by version control

---

## 3. Package Signing

### Signing model (V2+)
- Publishers can sign packages with Ed25519 keys
- Signatures are included in the package archive
- Registry verifies signatures for trusted publishers
- Consumers can require signatures for all dependencies

### Key management
- Each publisher has a signing key pair
- Public key is published with the publisher's account
- Private key is held by the publisher (never shared)
- Key rotation is supported (old signatures remain valid)

### Signature format

```
package: <archive-hash>
signer: <publisher-id>
signature: <ed25519-signature>
timestamp: <unix-timestamp>
```

### Trust model
- **Trusted publishers:** Packages from known publishers are trusted
- **Untrusted packages:** Packages from unknown publishers require explicit opt-in
- **Signature verification:** Optional but recommended for all dependencies

---

## 4. Trusted Publishers

### Publisher identity
- Publishers register with the registry
- Each publisher has a unique ID and public key
- Publisher reputation is tracked (download count, age, audit history)

### Trust levels
1. **Verified publisher:** Identity verified, signature valid
2. **Unverified publisher:** Identity not verified, signature may be valid
3. **Unknown publisher:** No publisher information

### Trust configuration

```toml
# mink.toml
[security]
trusted_publishers = ["acme-corp", "mink-team"]
require_signatures = true
allow_untrusted = false
```

---

## 5. Dependency Verification

### Verification levels

| Level | Description | Default |
|-------|-------------|---------|
| **Hash only** | Verify content hash | Yes |
| **Signature** | Verify publisher signature | No (opt-in) |
| **Audit** | Check for known vulnerabilities | Yes |
| **Full** | Hash + signature + audit | No (opt-in) |

### Verification commands

```bash
mink verify              # Verify all dependencies
mink verify --strict     # Require signatures
mink audit               # Check for vulnerabilities
mink audit --fix         # Suggest fixes
```

---

## 6. Lockfile Integrity

### Lockfile as security boundary
- The lockfile records exact versions and hashes
- Changing the lockfile changes what gets installed
- The lockfile is the single source of truth for dependency versions

### Lockfile protection
- Lockfile is committed to version control
- Changes to the lockfile are reviewed in code review
- CI verifies the lockfile is up-to-date
- `mink install --locked` fails if the lockfile is out of date

### Lockfile tampering detection
- Hash mismatch → error
- Missing package → error
- Version mismatch → error

---

## 7. Reproducible Builds

### Guarantees
1. Same `mink.lock` → same dependency versions
2. Same compiler version → same compilation behavior
3. Deterministic compilation (no HashMap ordering leakage)
4. Deterministic file paths (no absolute paths in output)

### Build verification

```bash
mink verify-build        # Verify build is reproducible
mink diff-build <hash>   # Compare two builds
```

### Build attestation (V2+)
- Each build produces an attestation (signed build record)
- Attestation includes: compiler version, dependency versions, build flags
- Attestations can be verified by consumers

---

## 8. Malicious Package Detection

### Detection methods

| Method | Description | When |
|--------|-------------|------|
| **Hash verification** | Verify content hash | Every download |
| **Signature verification** | Verify publisher signature | When enabled |
| **Vulnerability scanning** | Check for known CVEs | `mink audit` |
| **Behavioral analysis** | Detect suspicious build script behavior | V2+ |
| **Name similarity** | Detect typosquatting | Registry-side |
| **Dependency confusion** | Detect namespace shadowing | Registry-side |

### Suspicious behavior detection
- Build scripts that access the network without permission
- Build scripts that modify files outside the package directory
- Build scripts that execute external commands
- Packages with unusually high dependency counts
- Packages with names similar to popular packages

---

## 9. Build Script Restrictions

### Sandboxing model (V2+)

| Permission | Default | Opt-in |
|------------|---------|--------|
| Read package directory | Yes | — |
| Write package directory | Yes | — |
| Read system files | No | Yes |
| Write system files | No | No |
| Network access | No | Yes |
| Execute external commands | No | Yes |
| Access environment variables | Sanitized | Full access |

### Build script contract

```toml
# mink.toml
[build]
network = false          # Allow network access
sandbox = true           # Enable sandboxing
allowed_commands = []    # Allowed external commands
```

### Sandbox implementation
- Build scripts run in a restricted environment
- Filesystem access is limited to the package directory
- Network access is blocked by default
- Environment variables are sanitized (only PATH, HOME, etc.)
- Process creation is restricted

---

## 10. Credential Handling

### API tokens
- Registry authentication uses API tokens
- Tokens are stored in `~/.mink/credentials` (not in `mink.toml`)
- Tokens are never committed to version control
- Tokens can be revoked and rotated

### Token storage

```
~/.mink/credentials
machine mink.pkg.dev
login api-token
password <token>
```

### Token security
- Tokens are stored with restricted file permissions (0600)
- Tokens are never logged or displayed
- Tokens are sent over HTTPS only
- Token expiration is enforced

---

## 11. Secret Scanning

### Pre-publish scanning
- `mink publish` scans for secrets before uploading
- Detects: API keys, passwords, private keys, tokens
- Blocks publishing if secrets are detected

### Detection patterns
- AWS access keys (`AKIA...`)
- GitHub tokens (`ghp_...`)
- Private keys (`-----BEGIN.*PRIVATE KEY-----`)
- Generic high-entropy strings in sensitive locations

---

## 12. Registry Security

### HTTPS only
- All registry communication uses HTTPS
- Certificate pinning (V2+)
- HSTS headers

### Rate limiting
- 100 requests/minute per IP for reads
- 10 requests/minute per token for writes
- Exponential backoff on rate limit

### Input validation
- Package names are validated (lowercase, hyphens only)
- Versions are validated (semver format)
- Metadata is sanitized (no HTML, no scripts)

---

## 13. Package Takeover Prevention

### Namespace protection
- Package names are unique and immutable
- Publishers cannot transfer packages (V1)
- Package deletion is permanent (no undelete)

### Ownership verification
- Publishers must verify email before publishing
- Package ownership is tied to the publisher's account
- Account recovery requires email verification

### Yanking
- Publishers can yank versions (mark as unavailable)
- Yanked versions are not downloaded by default
- Yanked versions can be explicitly requested

---

## 14. Dependency Confusion Protection

### Namespace isolation
- Packages are identified by their full name (including namespace)
- Namespace `@org/package` is owned by the organization
- No namespace shadowing (different publishers cannot use the same namespace)

### Source priority
- Local packages (path dependencies) are always preferred
- Registry packages are used only when no local package exists
- Private registries can be configured per-namespace

### Configuration

```toml
# mink.toml
[registries]
mink = "https://mink.pkg.dev"
private = "https://packages.mycompany.com"

[dependencies]
my-lib = { version = "1.0.0", registry = "private" }
```

---

## 15. Typosquatting Mitigation

### Registry-side detection
- Similarity scoring between package names
- Warning for packages with names similar to popular packages
- Manual review for suspicious registrations

### Consumer-side protection
- `mink add` warns on similar package names
- `mink audit` checks for typosquatting
- Lockfile records the exact package name (no ambiguity)

### Similarity detection
- Levenshtein distance
- Soundex/phonetic matching
- Common typo patterns (e.g., `mink-std` vs `mink-std2`)

---

## 16. Audit Workflow

### Audit command

```bash
mink audit
```

### Output

```
Scanning 12 dependencies...

warning: advisory for mink-crypto 0.1.0
  advisory: MINKSEC-2026-0001
  title: Timing attack in HMAC verification
  severity: medium
  recommendation: upgrade to mink-crypto 0.2.0
  url: https://mink.pkg.dev/advisories/MINKSEC-2026-0001

warning: unpublished dependency my-experimental 0.1.0
  this package has no publisher verification
  consider requiring signatures for this dependency

2 warnings found.
```

### Audit integration
- CI runs `mink audit` on every build
- `mink publish` runs audit before publishing
- GitHub/GitLab integration for security alerts

---

## 17. Vulnerability Reporting

### Reporting process
1. Discover vulnerability
2. Report via secure channel (email, form)
3. Triaged by security team
4. Advisory published
5. Fix released
6. Users notified

### Advisory format

```json
{
  "id": "MINKSEC-2026-0001",
  "title": "Timing attack in HMAC verification",
  "severity": "medium",
  "package": "mink-crypto",
  "versions": ["<0.2.0"],
  "patched": "0.2.0",
  "description": "...",
  "recommendation": "Upgrade to mink-crypto 0.2.0"
}
```

### Disclosure policy
- 90-day disclosure deadline
- Coordinated disclosure with package maintainers
- Public disclosure after fix is available

---

## 18. Security Advisories

### Advisory database
- Maintained by the MINK security team
- Published at `https://mink.pkg.dev/advisories`
- Machine-readable (JSON)
- Integrated with `mink audit`

### Advisory subscription

```bash
mink audit --subscribe    # Subscribe to security alerts
```

---

## 19. Two-Approach Analysis

### Approach A: Proactive security design (CHOSEN)

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Security from day one, no retrofitting, user trust |
| **Cons** | Slower initial development, more upfront design |
| **Complexity** | Medium |
| **Performance** | Good (verification is fast) |
| **Security** | Excellent — designed for security |
| **Compatibility** | Good — security is transparent to users |
| **Maintainability** | Good — security is part of the architecture |
| **Ecosystem impact** | Maximum — users trust the ecosystem |

### Approach B: Reactive security

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Faster initial development, ship first |
| **Cons** | Security holes discovered after packages exist, retrofitting is expensive |
| **Complexity** | High (retrofitting security is harder than designing it) |
| **Performance** | Good initially, poor after retrofitting |
| **Security** | Poor — gaps discovered too late |
| **Compatibility** | Poor — breaking changes needed for security |
| **Maintainability** | Poor — security debt accumulates |
| **Ecosystem impact** | Negative — security incidents destroy trust |

### Decision: **Approach A**

**Reasoning:** Security must be designed BEFORE the package ecosystem grows. Retrofitting security after packages exist is expensive and destroys user trust. The proactive approach is slower initially but prevents catastrophic security incidents.

---

*This specification is part of the MINK Ecosystem Architecture Design Pack (Session 50).*
*Do NOT implement until the design is frozen and reviewed.*
