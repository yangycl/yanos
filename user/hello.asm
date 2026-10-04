; YEXE: magic, entry offset 8, then a single ret.
; nasm -f bin hello.asm -o HELLO.YEX
db "YEXE"
dd 8
ret
