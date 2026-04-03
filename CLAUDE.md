# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Facter is a system fact-collection tool. This repository contains:
- **Native Facter** (C++11) — the primary implementation on the `3.x` branch
- **Facter 4** (Ruby) — maintained on the `4.x` branch
- **factergo/** — a Go implementation (minimal, experimental)

The current working branch is `rust`. The main release branch is `3.x`.

## Build

All commands from repo root unless noted.

```bash
# Configure (debug build recommended for development)
mkdir debug && cd debug
cmake -DCMAKE_BUILD_TYPE=Debug ..

# Build
cd debug && make

# Run the built binary
./debug/bin/facter
```

On Windows, add `-G "MinGW Makefiles" -DCMAKE_PREFIX_PATH=<install_path> -DBOOST_STATIC=ON` to cmake.

## Testing

```bash
# Install Ruby test dependencies (only needed once if Ruby was found during cmake)
bundle install --gemfile lib/Gemfile

# Run all C++ tests
cd debug && make test

# Verbose C++ test output
cd debug && ctest -V

# Run a single C++ test by name
cd debug && ctest -V -R <test_name>

# Run Ruby specs (from repo root)
cd lib && bundle exec rspec spec/unit
```

## Linting / Static Analysis

```bash
cd debug
make cpplint    # C++ code style (requires python2 in PATH)
make cppcheck   # Static analysis (requires cppcheck in PATH)
```

## Architecture

Facter resolves system facts via a **Resolver** plugin pattern:

- `lib/inc/facter/` — Public API headers (`collection.hpp`, `fact.hpp`, `value.hpp`, `resolver.hpp`)
- `lib/inc/internal/` — Internal implementation headers
- `lib/src/facts/resolvers/` — One `.cc` per fact domain (networking, memory, OS, disk, cloud, virtualization, etc.)
- `lib/src/facts/{posix,linux,windows,osx,solaris,aix,bsd}/` — Platform-specific resolver implementations
- `lib/src/facts/external/` — External fact formats: JSON, YAML, TXT (key=value), executables
- `lib/src/ruby/` — Ruby bindings enabling `require 'facter'` and custom facts API
- `lib/schema/facter.yaml` — Fact schema; **all new facts must be added here** or acceptance tests will fail
- `exe/` — CLI entry point
- `acceptance/` — Beaker-based acceptance tests

### Fact Resolution Flow

1. `collection` aggregates all resolvers
2. Each resolver inherits from `base_resolver` and implements `resolve()`
3. Facts are typed: scalar, array, or map values
4. Facts can be cached with TTLs via HOCON config (`/etc/puppetlabs/facter/facter.conf`)

## Commit Message Format

```
(FACT-1234) Imperative description of change

Body explaining why the change is needed.
```

For trivial changes without a Jira ticket: prefix with `(docs)`, `(maint)`, or `(packaging)`.

## Adding New Facts

1. Implement resolver in `lib/src/facts/resolvers/` (and platform subdirs as needed)
2. Add the fact to `lib/schema/facter.yaml` — required for acceptance tests to pass
3. Add corresponding tests in `lib/tests/`
