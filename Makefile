PROG := $(shell grep '^name' Cargo.toml | sed -n 's/name = "\(.*\)"/\1/p')

DEBUG ?= 0

ifeq ($(DEBUG),1)
  RELEASE_FLAG :=
  TARGET_DIR := debug
  EXTENSION := -debug
else
  RELEASE_FLAG := --release
  TARGET_DIR := release
  EXTENSION :=
endif

PREFIX ?= /usr/local

BINDIR ?= $(PREFIX)/bin

CARGO ?= cargo

# PHONY targets are not files.
.PHONY: build install all help

# Default target is all.
default: all

build:
	$(CARGO) build $(RELEASE_FLAG)

install: build
	@mkdir -p $(BINDIR)
	@cp target/$(TARGET_DIR)/$(PROG) $(BINDIR)/$(PROG)$(EXTENSION)

lint:
	$(CARGO) clippy -- -D warnings
	$(CARGO) fmt --all -- --check

all: build install

help:
	@echo "Usage:"
	@echo "  make [DEBUG=1]"
	@echo "Options:"
	@echo "  DEBUG=1    Build the debug version of the binary."
