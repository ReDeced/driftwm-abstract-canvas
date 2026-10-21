# Makefile for driftwm-abstract-canvas (driftwm-wallpaper)
#
#   make           # build release binary
#   make install   # install binary to $(PREFIX)/bin
#   make uninstall # remove installed binary
#   make test      # run the test suite
#   make check     # cargo check
#   make fmt       # cargo fmt
#   make clean     # cargo clean

CARGO    ?= cargo
PREFIX   ?= /usr/local
DESTDIR  ?=
BINARY   := target/release/driftwm-wallpaper
INSTALL_NAME := driftwm-abstract-canvas
SRC_FILES := $(wildcard src/*.rs src/shaders/*.wgsl Cargo.toml)

.PHONY: all install uninstall test check fmt clean help

all: $(BINARY)

$(BINARY): $(SRC_FILES)
	$(CARGO) build --release

install: $(BINARY)
	install -Dm755 $(BINARY) $(DESTDIR)$(PREFIX)/bin/$(INSTALL_NAME)

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(INSTALL_NAME)

test:
	$(CARGO) test

check:
	$(CARGO) check

fmt:
	$(CARGO) fmt

clean:
	$(CARGO) clean

help:
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  all       Build the release binary (default)"
	@echo "  install   Install $(INSTALL_NAME) to \$$(PREFIX)/bin"
	@echo "  uninstall Remove \$$(PREFIX)/bin/$(INSTALL_NAME)"
	@echo "  test      Run cargo test"
	@echo "  check     Run cargo check"
	@echo "  fmt       Run cargo fmt"
	@echo "  clean     Run cargo clean"
