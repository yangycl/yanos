# ============================================================
# yasys / nova_os Makefile
# ============================================================

TOOLCHAIN := nightly-2026-08-01
TARGET    := x86_64-unknown-none

CARGO := cargo +$(TOOLCHAIN)

KERNEL_PACKAGE := nova_kernel
OS_PACKAGE     := nova_os

KERNEL_BIN := target/$(TARGET)/debug/$(KERNEL_PACKAGE)
OS_IMAGE   := os/nova_os.img

QEMU := qemu-system-x86_64
OVMF := $(firstword $(wildcard /usr/share/edk2/ovmf/OVMF_CODE.fd /usr/share/qemu/OVMF.fd /usr/share/OVMF/OVMF_CODE.fd))

ifeq ($(OVMF),)
$(error No OVMF BIOS found. Install ovmf package or set OVMF=/path/to/OVMF_CODE.fd)
endif

.PHONY: all setup check kernel image build run clean rebuild \
        toolchain targets

# Default target
all: build

# ------------------------------------------------------------
# Toolchain
# ------------------------------------------------------------

toolchain:
	rustup toolchain install $(TOOLCHAIN) --profile minimal

targets:
	rustup target add $(TARGET) x86_64-unknown-uefi \
		--toolchain $(TOOLCHAIN)

setup: toolchain
	rustup component add llvm-tools-preview rust-src \
		--toolchain $(TOOLCHAIN)
	rustup target add $(TARGET) x86_64-unknown-uefi \
		--toolchain $(TOOLCHAIN)

# ------------------------------------------------------------
# Check
# ------------------------------------------------------------

check:
	$(CARGO) check -p $(KERNEL_PACKAGE) --target $(TARGET)

# ------------------------------------------------------------
# Build kernel
# ------------------------------------------------------------

kernel:
	$(CARGO) build -p $(KERNEL_PACKAGE) --target $(TARGET)

# ------------------------------------------------------------
# Build bootable OS image
# ------------------------------------------------------------

image: kernel
	$(CARGO) build -p $(OS_PACKAGE)

# ------------------------------------------------------------
# Full build
# ------------------------------------------------------------

build: image

# ------------------------------------------------------------
# Run in QEMU
# ------------------------------------------------------------

run: image
	qemu-system-x86_64 \
	-drive format=raw,file=os/nova_os.img \
	-bios $(OVMF) \
	-m 256M \
	-serial stdio \
	-display gtk
# ------------------------------------------------------------
# Clean
# ------------------------------------------------------------

clean:
	$(CARGO) clean
	rm -f $(OS_IMAGE)

# ------------------------------------------------------------
# Rebuild from scratch
# ------------------------------------------------------------

rebuild: clean build
