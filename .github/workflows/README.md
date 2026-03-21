# GitHub Actions Workflows

This directory contains automated CI/CD workflows for the sketch project.

## Workflows

### `test.yml` - Continuous Integration

**Triggers:** Push to `main`, Pull Requests

**Jobs:**

1. **test-non-root** - Run tests as regular user
   - All unit tests and non-root-compatible integration tests
   - Runs clippy for linting
   - Expects: ✅ Pass

2. **test-root** - Run tests with root access
   - Overlay isolation tests (requires root)
   - Non-root tests still work
   - Expects: ⚠️ Can fail on systems without sufficient privileges (continues)

3. **build** - Build check and formatting
   - `cargo fmt` check
   - Build release binary
   - Verify `--help` and `--version` work
   - Expects: ✅ Pass

4. **security-audit** - Dependency security audit
   - Uses `rustsec/audit-check-action`
   - Checks for known vulnerabilities
   - Expects: ✅ Pass (warns on vulnerabilities)

5. **coverage** - Code coverage
   - Generates coverage report with `cargo-tarpaulin`
   - Uploads to Codecov
   - Expects: ℹ️ Informational

6. **docs** - Documentation check
   - Builds documentation with `cargo doc`
   - Runs doc tests
   - Expects: ✅ Pass

7. **test-summary** - Summary of all results
   - Displays results table
   - Fails if non-root or build checks fail
   - Root test failures don't fail the workflow

### `release.yml` - Release Pipeline

**Triggers:** Push of git tags matching `v*` (e.g., `v0.1.0`)

**Jobs:**

1. **build-release** - Build binaries for multiple platforms
   - Targets:
     - Linux x86_64
     - Linux ARM64 (aarch64)
     - macOS x86_64
     - macOS ARM64
   - Builds release binary
   - Creates SHA256 checksums

2. **create-release** - Create GitHub Release
   - Downloads all built artifacts
   - Extracts changelog for version
   - Creates GitHub Release with binaries
   - Assets are automatically downloadable

3. **publish-crate** - Publish to crates.io
   - Runs after release is created
   - Publishes to registry with `cargo publish`
   - Requires `CARGO_TOKEN` secret

## Configuration

### Required Secrets

For the release workflow to work, configure these secrets in GitHub repository settings:

- **`CARGO_TOKEN`** - API token for crates.io
  - Get from: https://crates.io/me
  - Permission: publish-new

### Optional Configuration

- **`CODECOV_TOKEN`** - For private repositories (optional)
  - Get from: https://codecov.io

## Setting Up Releases

### 1. Configure Cargo.toml

Ensure your `Cargo.toml` has:
```toml
[package]
name = "sketch"
version = "0.1.0"  # Must match tag
description = "..."
repository = "https://github.com/user/sketch"
```

### 2. Create Release Tag

```bash
git tag v0.1.0
git push origin v0.1.0
```

### 3. GitHub Actions Automatically

- ✅ Builds binaries for all platforms
- ✅ Creates GitHub Release
- ✅ Publishes to crates.io

## Test Results Interpretation

### Non-Root Tests (✅ Must Pass)

These run in the standard GitHub Actions environment as a regular user:
- Unit tests from `src/` modules
- CLI parsing tests
- Session tests (non-root behavior)
- Help/version tests

### Root Tests (⚠️ Can Fail)

These require root and overlay mount support:
- Overlay filesystem isolation tests
- Mount verification tests
- May fail if Ubuntu runner doesn't have full overlay support
- Workflow continues despite failures (expected)

**Note:** To verify these work locally, use:
```bash
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test
```

## Monitoring

### View Workflow Status

- GitHub Actions tab: https://github.com/user/sketch/actions
- Status badge in README

### Add Status Badge

```markdown
[![Tests](https://github.com/user/sketch/actions/workflows/test.yml/badge.svg)](https://github.com/user/sketch/actions/workflows/test.yml)
```

### View Coverage

Coverage reports are uploaded to Codecov:
- View: https://codecov.io/gh/user/sketch
- Add badge:
```markdown
[![codecov](https://codecov.io/gh/user/sketch/branch/main/graph/badge.svg)](https://codecov.io/gh/user/sketch)
```

## Troubleshooting

### Release Build Fails

1. Check `Cargo.toml` version matches tag
2. Ensure `CARGO_TOKEN` is set correctly
3. Check for documentation build issues with `cargo doc`

### Root Tests Fail in CI

This is expected on some systems. The workflow continues rather than failing because:
- Root overlay tests are "nice to have"
- Some CI environments don't support unprivileged overlayfs
- Non-root tests still validate most functionality

To skip root tests, set environment variable:
```bash
SKIP_ROOT_TESTS=1
```

### Coverage Not Uploading

- Coverage upload is non-blocking (continues on error)
- Codecov token not needed for public repos
- Check codecov.io dashboard for details

## Future Enhancements

Possible improvements:
- [ ] Cross-compilation for Windows
- [ ] Docker image builds and pushes
- [ ] Automated dependency updates (dependabot)
- [ ] Performance benchmarks
- [ ] Integration with other platforms (amd64, arm32)
