; YEXE: magic, .text length, then code.
; nasm -f bin hello.asm -o HELLO.YEX
db "YEXE"
dd text_end - text
text:
    ret
text_end:
