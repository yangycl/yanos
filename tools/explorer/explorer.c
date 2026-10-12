#include "syscalls.h"

typedef unsigned char u8;
typedef unsigned long usize;

#define ENTRY_SIZE 32
#define MAX_ENTRIES 16
#define PATH_CAPACITY 128
#define ROWS 12

/* Keep large buffers out of the kernel stack — running YEX on the kernel
 * stack with ~1.5KB of locals easily overflows and double-faults on return
 * from int 0x80. Non-zero init forces .data (BSS is discarded by linker /
 * rejected by elf2yex). Zeroed once at startup. */
static u8 g_entries[MAX_ENTRIES * ENTRY_SIZE] = {1};
static u8 g_visible[MAX_ENTRIES] = {1};
static char g_path[PATH_CAPACITY] = {1};
static char g_contents[513] = {1};
static char g_full_path[PATH_CAPACITY] = {1};

static usize string_length(const char *text) {
    usize length = 0;
    while (text[length] != '\0') {
        ++length;
    }
    return length;
}

static void copy_text(char *destination, const char *source, usize length) {
    for (usize i = 0; i < length; ++i) {
        destination[i] = source[i];
    }
}

static void draw_line(usize row, const char *text, u8 red, u8 green, u8 blue) {
    yanos_draw_text(24, 64 + row * 18, text, red, green, blue);
}

static void make_entry_name(const u8 *entry, char *name) {
    usize out = 0;
    usize base_length = 8;
    usize extension_length = 3;

    while (base_length > 0 && entry[base_length - 1] == ' ') {
        --base_length;
    }
    while (extension_length > 0 && entry[8 + extension_length - 1] == ' ') {
        --extension_length;
    }

    if (base_length == 1 && entry[0] == '.') {
        name[0] = '.';
        name[1] = '\0';
        return;
    }
    if (base_length == 2 && entry[0] == '.' && entry[1] == '.') {
        name[0] = '.';
        name[1] = '.';
        name[2] = '\0';
        return;
    }

    for (usize i = 0; i < base_length; ++i) {
        name[out++] = (char)entry[i];
    }
    if (extension_length != 0) {
        name[out++] = '.';
        for (usize i = 0; i < extension_length; ++i) {
            name[out++] = (char)entry[8 + i];
        }
    }
    name[out] = '\0';
}

static int is_visible(const u8 *entry) {
    u8 attributes = entry[11];
    return entry[0] != 0x00 && entry[0] != 0xE5
        && attributes != 0x0F && (attributes & 0x08) == 0;
}

static int read_entries(const char *path, u8 *entries, u8 *visible) {
    usize count = yanos_list_directory(path, string_length(path), entries,
                                       MAX_ENTRIES * ENTRY_SIZE);
    int visible_count = 0;
    for (usize i = 0; i < count && i < MAX_ENTRIES; ++i) {
        u8 *entry = entries + i * ENTRY_SIZE;
        if (is_visible(entry)) {
            visible[visible_count++] = (u8)i;
        }
    }
    return visible_count;
}

static void draw_directory(const char *path, const u8 *entries,
                           const u8 *visible, int count, int selected) {
    char line[48];
    char name[13];
    usize path_length = string_length(path);

    yanos_draw_rect(12, 12, 600, 300, 24, 32, 48);
    draw_line(0, "YANOS FILE EXPLORER", 100, 220, 255);
    draw_line(1, path_length == 0 ? "/" : path, 210, 210, 210);

    if (count == 0) {
        draw_line(3, "Empty directory or syscall unavailable", 255, 180, 100);
    }

    for (int row = 0; row < count && row < ROWS; ++row) {
        const u8 *entry = entries + (usize)visible[row] * ENTRY_SIZE;
        make_entry_name(entry, name);
        usize length = 0;
        line[length++] = row == selected ? '>' : ' ';
        line[length++] = ' ';
        for (usize i = 0; name[i] != '\0' && length < sizeof(line) - 3; ++i) {
            line[length++] = name[i];
        }
        if ((entry[11] & 0x10) != 0) {
            line[length++] = '/';
        }
        line[length] = '\0';
        draw_line((usize)row + 3, line,
                  row == selected ? 255 : 220,
                  row == selected ? 230 : 220,
                  row == selected ? 120 : 220);
    }

    draw_line(ROWS + 3, "UP/DOWN select  ENTER open  BACKSPACE parent  ESC quit",
              150, 180, 150);
}

static void draw_file(const char *name, const char *contents) {
    char line[65];
    usize offset = 0;
    usize row = 2;
    yanos_draw_rect(12, 12, 600, 300, 24, 32, 48);
    draw_line(0, "YANOS FILE VIEWER", 100, 220, 255);
    draw_line(1, name, 210, 210, 210);
    while (contents[offset] != '\0' && row < ROWS + 2) {
        usize length = 0;
        while (contents[offset] != '\0' && contents[offset] != '\n'
               && length < sizeof(line) - 1) {
            line[length++] = contents[offset++];
        }
        if (contents[offset] == '\n') {
            ++offset;
        }
        line[length] = '\0';
        draw_line(row++, line, 220, 220, 220);
    }
    draw_line(ROWS + 3, "ESC or BACKSPACE to return", 150, 180, 150);
}

static void parent_directory(char *path) {
    usize length = string_length(path);
    while (length > 0 && path[length - 1] != '/') {
        --length;
    }
    if (length > 0) {
        --length;
    }
    path[length] = '\0';
}

void explorer_main(void) {
    int selected = 0;
    int extended_key = 0;
    usize i;

    /* Clear the forced non-zero .data markers. */
    for (i = 0; i < sizeof(g_entries); ++i) {
        g_entries[i] = 0;
    }
    for (i = 0; i < sizeof(g_visible); ++i) {
        g_visible[i] = 0;
    }
    for (i = 0; i < sizeof(g_path); ++i) {
        g_path[i] = 0;
    }
    for (i = 0; i < sizeof(g_contents); ++i) {
        g_contents[i] = 0;
    }
    for (i = 0; i < sizeof(g_full_path); ++i) {
        g_full_path[i] = 0;
    }

    for (;;) {
        int count = read_entries(g_path, g_entries, g_visible);
        if (count == 0) {
            selected = 0;
        } else if (selected >= count) {
            selected = count - 1;
        }
        draw_directory(g_path, g_entries, g_visible, count, selected);

        usize key = yanos_get_key();
        if (key == 0) {
            continue;
        }
        if (extended_key) {
            extended_key = 0;
            if (key == 0x48 && selected > 0) {
                --selected;
            } else if (key == 0x50 && selected + 1 < count) {
                ++selected;
            }
            continue;
        }
        if (key == 0xE0) {
            extended_key = 1;
        } else if (key == 0x01) {
            return;
        } else if (key == 0x0E) {
            parent_directory(g_path);
            selected = 0;
        } else if (key == 0x1C && count > 0) {
            const u8 *entry = g_entries + (usize)g_visible[selected] * ENTRY_SIZE;
            char name[13];
            make_entry_name(entry, name);
            if ((entry[11] & 0x10) != 0) {
                usize length = string_length(g_path);
                usize name_length = string_length(name);
                usize needed = length + (length != 0) + name_length + 1;
                if (needed <= PATH_CAPACITY) {
                    if (length != 0) {
                        g_path[length++] = '/';
                    }
                    copy_text(g_path + length, name, name_length + 1);
                    selected = 0;
                }
            } else {
                usize length = string_length(g_path);
                if (length + (length != 0) + string_length(name) + 1 <= PATH_CAPACITY) {
                    copy_text(g_full_path, g_path, length);
                    if (length != 0) {
                        g_full_path[length++] = '/';
                    }
                    copy_text(g_full_path + length, name, string_length(name) + 1);
                    usize bytes = yanos_read_file(g_full_path, string_length(g_full_path),
                                                  g_contents, sizeof(g_contents) - 1);
                    g_contents[bytes] = '\0';
                    for (usize i = 0; i < bytes; ++i) {
                        u8 ch = (u8)g_contents[i];
                        if (ch < 0x20 && ch != '\n' && ch != '\t') {
                            g_contents[i] = ' ';
                        }
                    }
                    draw_file(name, g_contents);
                    for (;;) {
                        key = yanos_get_key();
                        if (key == 0x01 || key == 0x0E) {
                            break;
                        }
                    }
                }
            }
        }
    }
}
