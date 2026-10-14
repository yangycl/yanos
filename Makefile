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

.PHONY: all setup check kernel image build run clean rebuild reCpToEfi \
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

# ------------------------------------------------------------
# Copy the kernel from the freshly built image to host EFI
# ------------------------------------------------------------

reCpToEfi: rebuild
	@set -eu; \
	if ! mountpoint -q /boot/efi; then \
		echo "Error: /boot/efi is not mounted" >&2; exit 1; \
	fi; \
	if mountpoint -q /mnt/img; then \
		echo "Error: /mnt/img is already mounted; refusing to replace its mount" >&2; exit 1; \
	fi; \
	sudo mkdir -p /mnt/img; \
	sudo mount -o loop,ro,offset=17408 "$(OS_IMAGE)" /mnt/img; \
	cleanup() { sudo umount /mnt/img; }; \
	trap cleanup EXIT INT TERM; \
	test -f /mnt/img/kernel-x86_64 || { echo "Error: kernel-x86_64 missing from $(OS_IMAGE)" >&2; exit 1; }; \
	sudo install -D -m 0644 /mnt/img/kernel-x86_64 /boot/efi/EFI/yanos/kernel-x86_64; \
	src_hash=$$(sha256sum /mnt/img/kernel-x86_64 | cut -d' ' -f1); \
	dst_hash=$$(sudo sha256sum /boot/efi/EFI/yanos/kernel-x86_64 | cut -d' ' -f1); \
	echo "Image kernel: $$src_hash"; \
	echo "EFI kernel:   $$dst_hash"; \
	test "$$src_hash" = "$$dst_hash"; \
	echo "Kernel copied and verified in /boot/efi/EFI/yanos/kernel-x86_64"


Disassemble:
	~/.rustup/toolchains/nightly-2026-08-01-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-objdump -d -S target/x86_64-unknown-none/debug/nova_kernel > kernel.asm
