EXPLORER_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
ROOT := $(abspath $(EXPLORER_DIR)/../..)
BUILD_DIR := $(EXPLORER_DIR)/build
CC ?= gcc
LD ?= ld
NASM ?= nasm
CARGO ?= cargo

CFLAGS := -m64 -O2 -Wall -Wextra -ffreestanding -fPIC -fno-plt \
          -fno-stack-protector -fno-builtin -fno-ident \
          -fno-asynchronous-unwind-tables -fno-unwind-tables \
          -mno-red-zone -I$(EXPLORER_DIR)

.PHONY: all clean
all: $(BUILD_DIR)/EXPLORER.YEX

$(BUILD_DIR):
	mkdir -p $@

$(BUILD_DIR)/syscalls.o: $(EXPLORER_DIR)/syscalls.asm | $(BUILD_DIR)
	$(NASM) -f elf64 $< -o $@

$(BUILD_DIR)/explorer.o: $(EXPLORER_DIR)/explorer.c $(EXPLORER_DIR)/syscalls.h | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR)/explorer.elf: $(BUILD_DIR)/syscalls.o $(BUILD_DIR)/explorer.o $(EXPLORER_DIR)/linker.ld
	$(LD) -m elf_x86_64 -nostdlib --build-id=none \
		-T $(EXPLORER_DIR)/linker.ld -o $@ $(BUILD_DIR)/syscalls.o $(BUILD_DIR)/explorer.o

$(BUILD_DIR)/EXPLORER.YEX: $(BUILD_DIR)/explorer.elf
	cd $(ROOT) && $(CARGO) run -p elf2yex -- $(BUILD_DIR)/explorer.elf $(BUILD_DIR)/EXPLORER.YEX

clean:
	rm -rf $(BUILD_DIR)
