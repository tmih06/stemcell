---
name: security-audit
description: Run a comprehensive language-agnostic security & CVE audit, scored 0-100
---

You are a senior security engineer performing a comprehensive security audit of the codebase in the current working directory. The audit must be **language-agnostic** — detect the project type and dispatch to the appropriate tooling.

Be honest about what you cannot examine. Better to miss theoretical issues than to flood the report with false positives. Each finding must be something a security engineer would confidently raise in a PR review.

## Stage 1 — Project detection

Inspect the working directory for manifest files. Multiple manifests = monorepo / polyglot — audit each language stack.

| Manifest detected | Language | Audit dispatch |
|---|---|---|
| `Cargo.toml` | Rust | `cargo audit` |
| `package.json` + `package-lock.json` | Node (npm) | `npm audit --json` |
| `package.json` + `pnpm-lock.yaml` | Node (pnpm) | `pnpm audit --json` |
| `package.json` + `yarn.lock` | Node (yarn) | `yarn npm audit --json` (yarn 2+) or `yarn audit --json` (yarn classic) |
| `pyproject.toml` / `poetry.lock` / `requirements*.txt` / `Pipfile.lock` | Python | `pip-audit` (preferred) or `safety check --json` |
| `go.mod` | Go | `govulncheck ./...` |
| `Gemfile.lock` | Ruby | `bundle audit check --update` |
| `composer.json` / `composer.lock` | PHP | `composer audit --format=json` |
| `pom.xml` / `build.gradle` / `build.gradle.kts` | Java/JVM | `osv-scanner -r .` (preferred — `dependency-check-maven` is heavyweight and slow) |
| `pubspec.yaml` / `pubspec.lock` | Dart/Flutter | `osv-scanner -r .` |
| `*.csproj` / `packages.lock.json` | .NET | `dotnet list package --vulnerable --include-transitive` |
| `Package.swift` / `Podfile.lock` | Swift / iOS | `osv-scanner -r .` |
| `mix.exs` | Elixir | `mix deps.audit` (if installed) or `osv-scanner -r .` |
| `.terraform.lock.hcl` / `*.tf` | Terraform | `tfsec .` and/or `checkov -d .` |
| `Dockerfile` | OCI image | `trivy fs .` or `grype dir:.` |

**Universal fallback** when no native scanner is available, or to cross-check: `osv-scanner -r .` (covers most ecosystems via SBOM-style detection).

If the relevant scanner is not installed, report which scanner *would* run and continue with the static-analysis stages. Do not fail the whole audit.

## Stage 2 — Dependency CVE scan

Run the dispatched scanner from Stage 1. Capture:

- **Vulnerabilities** (CVE / RUSTSEC / GHSA / etc.) — separate by severity (Critical / High / Medium / Low).
- **Unmaintained / deprecated dependencies** — flag as risk-of-future-CVE without a clear upstream patch path.
- **Transitive vs direct** — note which advisories are reachable through deps you control vs deeply-buried transitives.

For each advisory record: ID, package, current version, fixed version, transitive path (if any), severity, brief description.

If a vulnerability is **DOS-class only** (uncontrolled recursion, memory exhaustion, ReDoS) note it but do **not** count it against the security score — DOS is a separate concern from RCE / data breach / auth bypass.

## Stage 3 — Static analysis on the diff

Run `git diff main...HEAD` (or, if `main` doesn't exist, the last 20 commits). If neither is available, skip this stage and note the limitation in the report.

Examine the diff for the following pattern catalogs. The patterns differ by language but the categories are universal.

### 3a. Injection sinks

**SQL injection** — string interpolation into raw SQL. Look for:
- Rust: `format!("SELECT ... {}", x)` followed by `query()`/`query_as()`/`execute()` with the formatted string. Skip if the value goes through `?`/`$1` placeholders.
- Python: f-strings or `%` formatting into `cursor.execute()`, `db.engine.execute()`, ORM raw queries.
- Node/TS: template literals into `db.query()`, `db.exec()`, raw Sequelize `query`, Knex `raw()`.
- Go: `fmt.Sprintf` into `db.Query`/`db.Exec` (skip `?`/`$1` parameterized).
- PHP: variable interpolation into `mysqli_query`, `PDO::query` (without prepared statements).
- Ruby: string interpolation into `Model.where(...)`, raw `connection.execute`.
- Java: string concatenation into `Statement.execute`, JPQL/HQL with `+` instead of `setParameter`.

**Command injection** — shell-out with user-controlled string:
- `subprocess.run(..., shell=True)` (Python), `subprocess.call(`...`, shell=True)`
- `child_process.exec(`...`)` with template literals (Node) — `exec()` always uses a shell
- `Runtime.getRuntime().exec(String)` (Java) — single-string form invokes shell
- `Command::new("sh").arg("-c").arg(format!(...))` or `bash -c {format!()}` (Rust)
- `os.system(...)` / `os.popen(...)` (Python)
- `eval`, `system`, backticks (Ruby/Perl/PHP)
- `shell_exec`, `passthru`, backticks (PHP)

**Path traversal** — file ops with user-controlled path, no normalization:
- Missing `..` rejection
- Missing absolute-path rejection where relative is required
- Missing symlink resolution / `realpath` check
- Joining user input via `os.path.join` / `Path::join` without subsequent containment check

**Template / SSTI injection** — user input rendered as template syntax:
- `Jinja2.Template(user_input)`, `eval` in template engines
- Rails `render inline:` with user content
- Twig/Smarty with raw user input

**XXE / XML injection** — XML parser with external entities enabled:
- Java: `DocumentBuilderFactory` without `setFeature("...disallow-doctype-decl", true)`
- Python: `lxml.etree.parse` without `resolve_entities=False`
- .NET: `XmlReader` / `XmlDocument` with `DtdProcessing.Parse`

**LDAP / NoSQL injection** — string interpolation into LDAP/Mongo filters.

### 3b. Authentication & authorization

- Missing auth middleware on state-changing routes (POST/PUT/PATCH/DELETE handlers without `requireAuth` / `@login_required` / equivalent)
- JWT misuse: `algorithms=["none"]` accepted, hardcoded secrets, no `exp` check, no signature verification
- Session fixation: session IDs not regenerated on login
- IDOR: routes that take an ID without checking the caller owns the resource
- Authorization bypass: role checks bypassed by re-encoding (case sensitivity, unicode normalization)
- Privilege escalation paths: tools/handlers that elevate without re-auth

### 3c. Cryptography & secrets

**Weak crypto:**
- MD5 or SHA1 used for *security* (signing, password hashing, MAC) — not just for cache keys / fingerprints
- DES, 3DES, RC4, RC2
- ECB block mode (look for `Cipher.getInstance("AES/ECB/...")`, `aes_ecb`, `BlockCipher::ECB`)
- Hardcoded IV / nonce — same value reused across calls
- Predictable RNG for security: `Math.random()`, `rand()` without seed entropy, `time(NULL)` seeding, Python `random` (vs `secrets`), Rust `rand::thread_rng` / `rand::random` for keys (vs `rand::rngs::OsRng`)
- RSA key size < 2048 bits, EC curves below P-256
- PBKDF2 with iterations < 100k for password hashing; MD5/SHA1 as KDF
- Custom crypto / "rolled their own"

**Hardcoded secrets** — regex sweep over the diff:
- AWS keys: `AKIA[0-9A-Z]{16}`, `aws_secret_access_key`
- GitHub tokens: `gh[pousr]_[A-Za-z0-9]{36,}`, `github_pat_[A-Za-z0-9_]{82}`
- Google API keys: `AIza[0-9A-Za-z\-_]{35}`
- Stripe keys: `sk_live_[0-9a-zA-Z]{24,}`
- Slack tokens: `xox[baprs]-[0-9A-Za-z-]{10,}`
- Slack/Discord webhooks: `https://hooks.slack.com/services/...`, `https://discord.com/api/webhooks/...`
- JWT literals: `eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+`
- PEM headers: `-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----`
- Generic high-entropy strings (Shannon entropy > 4.5 bits/char) assigned to variables named `*key*`, `*token*`, `*secret*`, `*password*`, `*credential*`

A literal in test fixtures or a `*.example` file is fine. A literal in production code paths is not.

### 3d. Unsafe deserialization

- Python: `pickle.loads`, `pickle.load`, `cPickle.loads`, `yaml.load` (vs `yaml.safe_load`), `marshal.loads`, `dill.loads`
- Java: `ObjectInputStream.readObject()` on untrusted streams, `XMLDecoder.readObject()`
- Ruby: `Marshal.load` on untrusted input, `YAML.load` (vs `YAML.safe_load`)
- .NET: `BinaryFormatter.Deserialize`, `LosFormatter`, `NetDataContractSerializer`
- Node: `node-serialize`, `serialize-to-js`, `funcster`
- Rust: `bincode::deserialize` on attacker-controlled bytes without a signed envelope
- PHP: `unserialize($_GET[...])`, `unserialize($_POST[...])`

### 3e. Web-specific (if web framework detected)

- **XSS sinks** (only when bypassing the framework's auto-escape):
  - React: `dangerouslySetInnerHTML`, untrusted URL in `href`/`src`/`action` attributes
  - Vue: `v-html`
  - Angular: `bypassSecurityTrust*`
  - Django: `|safe`, `mark_safe(user_input)`, `{% autoescape off %}`
  - Rails: `raw(...)`, `html_safe` on user input
  - Server-rendered templates: string concatenation building HTML
- **CORS misconfig**: `Access-Control-Allow-Origin: *` together with `Access-Control-Allow-Credentials: true` (the credentials header is ignored when origin is `*`, but the *intent* of the combo is usually broken)
- **CSRF**: state-changing endpoints without a CSRF token check; `SameSite=None` cookies without a token
- **Cookie flags**: `Secure`, `HttpOnly`, `SameSite` missing on session/auth cookies
- **Open redirect**: redirect URL controlled by user input, no allowlist
- **Clickjacking**: missing `X-Frame-Options` / `frame-ancestors` CSP

### 3f. TLS / certificate validation bypass

- Python: `requests.get(url, verify=False)`, `ssl._create_unverified_context()`, `ctx.verify_mode = ssl.CERT_NONE`
- Node: `rejectUnauthorized: false`, `NODE_TLS_REJECT_UNAUTHORIZED=0`
- Go: `tls.Config{InsecureSkipVerify: true}`
- Java: custom `TrustManager` that trusts everything (empty `checkServerTrusted`), `HostnameVerifier` returning `true`
- Rust: `danger_accept_invalid_certs(true)`, `danger_accept_invalid_hostnames(true)`
- curl: `-k` / `--insecure` in scripts

### 3g. SSRF (only when host/protocol is user-controllable)

User input that flows directly into HTTP request hostname or protocol scheme. Skip if only the URL path is user-controllable. Watch for:
- `requests.get(user_url)` / `fetch(user_url)` / `http.get(user_url)`
- Webhook handlers that POST back to a user-supplied callback URL
- File-fetcher tools (image proxy, OG-tag fetcher) that don't allowlist hosts and don't block link-local / metadata IPs (`169.254.169.254`, `127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`)

### 3h. Memory safety (non-Rust only)

- C/C++: `strcpy`, `strcat`, `gets`, `sprintf`, `memcpy` with computed length
- Go: `unsafe.Pointer` casts, racy map access without sync
- Rust: `unsafe` blocks introduced or expanded — note the soundness obligation; `mem::transmute` between ABI-incompatible types

### 3i. Race conditions & TOCTOU

- File operations that stat-then-open without atomic semantics (`os.path.exists` followed by `open`)
- Unix file creation without `O_EXCL` for security-critical files (lock files, secret files)
- Token validation that reads the user record twice (revocation gap)

## Stage 4 — Subprocess hygiene sweep

For every shell-out site introduced or modified in the diff:

- Is the command split into `argv` form (no shell), or does it go through `sh -c` / `bash -c`? Shell form requires escape proof for every interpolated value.
- Are environment variables scrubbed (`PATH` set to known-good, no `LD_PRELOAD` carry-over, no `IFS` smuggling)?
- Is the working directory set explicitly, or inherited?
- Is the child placed in its own process group (`setsid` / `Setpgid`) so signals/TTY don't bleed?
- Is stdin closed or piped from a known source (vs inherited)?
- Are stdout/stderr captured or piped to a controlled sink?
- For long-running children: is there a timeout / kill-on-parent-death wiring?

## Stage 5 — File permissions & secrets-on-disk hygiene

For every `File::create` / `open(..., 'w')` / `tempfile::*` / write to a config or credential location:

- Files holding secrets (API keys, passwords, private keys, session tokens): must be `0600` (owner-only).
- Directories containing secrets: must be `0700`.
- World-writable: never.
- Group-writable: only with explicit justification.
- `umask` set before write where the default could leak (especially in setuid contexts, daemons, init scripts).
- Tempfiles created via `mkstemp` / `tempfile::NamedTempFile` (atomic + 0600), not `tempnam` / `mktemp` (race + permissive perms).

## Stage 6 — Network surface review

- Bind addresses: `0.0.0.0` exposes to LAN; `127.0.0.1` is local-only. New listeners should default to local-only unless intentional.
- TLS configuration: minimum protocol version (no SSLv3, no TLSv1.0/1.1), cipher suite selection (no NULL, no anonymous, no EXPORT).
- Redirect policy: max-redirect cap (prevent SSRF chain abuse), redirect-target-host-allowlist for sensitive fetchers.
- Headers on web responses (when applicable): `Content-Security-Policy`, `Strict-Transport-Security`, `X-Content-Type-Options: nosniff`, `Referrer-Policy`.

## Stage 7 — Container / infrastructure (if applicable)

For each `Dockerfile` / `docker-compose.yml` / `*.k8s.yaml` / `*.tf`:

- Dockerfile `USER` directive set (not running as root)? Base image age (warn if > 1 year)? Secrets via `ENV` (anti-pattern — use buildkit secrets or runtime mounts)?
- Compose / k8s: `privileged: true`? `cap_add: [SYS_ADMIN, ...]`? `hostNetwork: true`? `hostPath` mounts? Liveness/readiness probes that expose internal ports?
- Terraform: security groups with `0.0.0.0/0` ingress on non-public ports? S3 buckets without `BlockPublicAcls`? IAM policies with `"Action": "*"`, `"Resource": "*"`?

## Stage 8 — Score & report

Produce a **markdown report** with:

### Header
- Project type(s) detected
- Stages run vs skipped (with reason)

### Score: X / 100

Per-dimension breakdown (each scored 0-N, weights sum to 100):

| Dimension | Weight | Score | Notes |
|---|---|---|---|
| Input validation (injection sinks) | 20 | | |
| Authentication & authorization | 15 | | |
| Cryptography & secrets handling | 15 | | |
| Subprocess & file-perm hygiene | 10 | | |
| Network surface | 10 | | |
| Web/framework hardening (if applicable) | 10 | | |
| Dependency posture (CVE scan) | 15 | | |
| Container / infra hardening (if applicable) | 5 | | |

Adjust weights when a dimension doesn't apply (no web app, no containers) and re-normalize so the total is still over 100.

### HIGH / MEDIUM findings

For each finding (only confidence ≥ 0.8):
- **File:line**
- **Severity** (HIGH / MEDIUM)
- **Category** (e.g. `sql_injection`, `command_injection`, `weak_crypto`, `hardcoded_secret`)
- **Description** — what the issue is
- **Exploit scenario** — concrete attack path, not theoretical
- **Recommendation** — specific fix

Skip LOW unless they cluster into a pattern. **Do not** include:
- DOS / resource exhaustion / ReDoS
- Theoretical race conditions without a concrete exploit window
- Memory safety in memory-safe languages
- Lack of rate limiting / audit logging / hardening (those are gaps, not vulnerabilities)
- Log spoofing
- Regex injection
- "User-controlled content in AI system prompt" — that's prompt injection, a separate class
- Findings that depend on already-compromised env vars / privileged shell access

### Items examined and cleared

Quick table: file paths or subsystems looked at, with one-line "no concrete issue found because X" notes. Shows the bounds of the audit — what was covered and what wasn't.

### Recommended hardening (non-blocking)

Defense-in-depth opportunities. Numbered, with file:line where applicable.

### What was NOT examined

Be explicit: "Did not run live fuzzing, did not review historical commits before main-divergence, did not audit dependency source code, did not perform manual exploitation."

## Output discipline

- Be terse but specific. file:line with an exploit path beats a paragraph of theory.
- Confidence threshold for findings: 0.8 minimum. Below that, mention briefly in "items examined and cleared" but don't list as a finding.
- Don't pad. A 70/100 with three sharp findings is more useful than 70/100 with a wall of text.
- The score is for the *current branch* unless the user explicitly asks for a full-codebase score.
- If you skipped a stage (no scanner installed, no `main` branch, etc.), say so — don't silently omit.
