BITS 64

section .text.start

global _start
extern explorer_main

_start:
    ; The kernel enters this function with a C ABI return address on the stack.
    sub rsp, 8
    call explorer_main
    add rsp, 8
    ret

section .text

global yanos_get_key
global yanos_read_file
global yanos_list_directory
global yanos_draw_text
global yanos_draw_rect

; unsigned long yanos_get_key(void)
yanos_get_key:
    push rbx
    mov eax, 3
    int 0x80
    pop rbx
    ret

; unsigned long yanos_read_file(const char *path, unsigned long path_len,
;                               void *destination, unsigned long capacity)
; SysV args: rdi, rsi, rdx, rcx
; syscall 4 args: rbx=path, rcx=path_len, rdx=destination, rsi=capacity
yanos_read_file:
    push rbx
    mov r10, rcx
    mov rbx, rdi
    mov rcx, rsi
    mov rsi, r10
    mov eax, 4
    int 0x80
    pop rbx
    ret

; unsigned long yanos_list_directory(const char *path, unsigned long path_len,
;                                    void *entries, unsigned long capacity)
yanos_list_directory:
    push rbx
    mov r10, rcx
    mov rbx, rdi
    mov rcx, rsi
    mov rsi, r10
    mov eax, 5
    int 0x80
    pop rbx
    ret

; void yanos_draw_text(unsigned long x, unsigned long y, const char *text,
;                      unsigned char red, unsigned char green, unsigned char blue)
; syscall 2 args: rbx=text, rcx=x, rdx=y, r8=red, r9=green, r10=blue
yanos_draw_text:
    push rbx
    mov r10, r9
    mov r9, r8
    mov r8, rcx
    mov rbx, rdx
    mov rcx, rdi
    mov rdx, rsi
    mov eax, 2
    int 0x80
    pop rbx
    ret

; void yanos_draw_rect(unsigned long x, unsigned long y,
;                      unsigned long width, unsigned long height,
;                      unsigned char red, unsigned char green,
;                      unsigned char blue)
; syscall 1 args: rbx=x, rcx=y, r8=width, r9=height,
;                 r10=red, r11=green, r12=blue
yanos_draw_rect:
    push rbx
    push r12
    mov r10, r8
    mov r11, r9
    mov r12, [rsp + 24]
    mov rbx, rdi
    mov r8, rdx
    mov r9, rcx
    mov rcx, rsi
    mov eax, 1
    int 0x80
    pop r12
    pop rbx
    ret

section .note.GNU-stack noalloc noexec nowrite progbits
