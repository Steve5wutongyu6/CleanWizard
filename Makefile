# CleanWizard — build entry points.
#
# Day to day:
#   make build     compile the Rust engine and the App (Debug, host arch)
#   make test      cargo test + xcodebuild test
#   make run       build and launch the App
#   make release   Release build, universal
#
# Every target shells out to scripts/ so CI and local builds stay identical.

SHELL := /bin/bash
ROOT  := $(shell pwd)
CONFIG ?= Debug
ARCHS  ?=

.PHONY: all build run test test-engine test-app release clean lint engine

all: build

# Build the engine first: the App links the static library produced here.
engine:
	@ARCH="$(shell uname -m)" ./scripts/build-rust.sh

build: engine
	@./scripts/build.sh --debug

run: build
	@open "$(ROOT)/build/$(CONFIG)/CleanWizard.app" && echo "launched CleanWizard"

test: test-engine test-app
	@echo "all tests passed"

test-engine:
	@cd engine && cargo test --locked

test-app:
	@./scripts/test.sh app

release:
	@./scripts/build.sh --release

lint:
	@./scripts/lint.sh

clean:
	@rm -rf build
	@cd engine && cargo clean
	@echo "cleaned"
