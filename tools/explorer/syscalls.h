#ifndef YANOS_EXPLORER_SYSCALLS_H
#define YANOS_EXPLORER_SYSCALLS_H

/* All paths are relative to the mounted FAT32 root. */
unsigned long yanos_get_key(void);
unsigned long yanos_read_file(const char *path, unsigned long path_len,
                              void *destination, unsigned long capacity);
unsigned long yanos_list_directory(const char *path, unsigned long path_len,
                                   void *entries, unsigned long capacity);
void yanos_draw_text(unsigned long x, unsigned long y, const char *text,
                     unsigned char red, unsigned char green,
                     unsigned char blue);
void yanos_draw_rect(unsigned long x, unsigned long y,
                     unsigned long width, unsigned long height,
                     unsigned char red, unsigned char green,
                     unsigned char blue);

#endif
