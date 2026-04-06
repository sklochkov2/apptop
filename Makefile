PREFIX ?= /usr/local
BINDIR  = $(PREFIX)/bin

.PHONY: build release install uninstall clean

build:
	cargo build

release:
	cargo build --release

install: release
	install -Dm755 target/release/apptop $(DESTDIR)$(BINDIR)/apptop

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/apptop

clean:
	cargo clean
