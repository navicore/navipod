# Navipod Build System
#
# This is the SOURCE OF TRUTH for all build/test/lint operations.
# GitHub Actions calls these recipes directly - no duplication!

# Default recipe: show available commands
default:
    @just --list

# Build release binary
build:
    @echo "Building release..."
    cargo build --release

# Run all tests
test:
    @echo "Running tests..."
    cargo test --lib
    cargo test --test cache_integration_test
    cargo test --test k8s_cache_integration

# Run clippy with strict lints (denies all warnings - blocks CI)
# Note: unwrap/expect are denied in lib code but allowed in tests
lint:
    @echo "Running strict clippy on library code..."
    cargo clippy --lib --all-features -- \
        -D warnings \
        -D clippy::pedantic \
        -D clippy::nursery \
        -D clippy::unwrap_used \
        -D clippy::expect_used
    @echo "Running clippy on tests (unwrap allowed)..."
    cargo clippy --tests --all-features -- \
        -D warnings \
        -D clippy::pedantic \
        -D clippy::nursery

# Format all code
fmt:
    @echo "Formatting code..."
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    @echo "Checking code formatting..."
    cargo fmt --all -- --check

# Run clippy and auto-fix what it can
clippy-fix:
    @echo "Running clippy with auto-fix..."
    cargo clippy --fix --allow-dirty --allow-staged -- \
        -W clippy::pedantic \
        -W clippy::nursery \
        -W clippy::unwrap_used \
        -W clippy::expect_used

# Run all CI checks (same as GitHub Actions!)
# This is what developers should run before pushing
ci: fmt-check lint test build
    @echo ""
    @echo "✅ All CI checks passed!"
    @echo "   - Code formatting ✓"
    @echo "   - Clippy lints ✓"
    @echo "   - Tests ✓"
    @echo "   - Build ✓"
    @echo ""
    @echo "Safe to push to GitHub - CI will pass."

# Quick development cycle: format + build + test
dev: fmt build test

# Clean build artifacts
clean:
    @echo "Cleaning build artifacts..."
    cargo clean

# Run cache-specific tests
cache-test:
    @echo "Running cache tests..."
    cargo test --lib k8s::cache
    cargo test --test cache_integration_test

# Show test output (verbose)
test-verbose:
    cargo test -- --nocapture

# Generate documentation
doc:
    cargo doc --no-deps --open
