/* Does an ARM926 raise an alignment fault for a doubleword transfer that is
 * four-byte aligned but not eight?
 *
 * QEMU's user mode emulates alignment faults and carries on, so it cannot
 * answer this; system mode raises the exception the architecture says to. Run
 * with `-M versatilepb` and this file at address zero; it writes one letter to
 * the PL011 at 0x101f1000.
 *
 *   A  alignment checking is on and the first store is about to happen
 *   B  the eight-byte-aligned store completed
 *   C  the four-byte-aligned store completed
 *   F  the data abort vector was taken, and FSR and FAR follow in hex
 *
 * `ABC` means four is enough. `ABF...` means a doubleword needs eight. */

    .text
    .arm
    .global _start

    .equ UART, 0x101f1000

    .org 0
    b       _start              @ 0x00 reset
    b       hang                @ 0x04 undefined
    b       hang                @ 0x08 supervisor call
    b       hang                @ 0x0c prefetch abort
    b       data_abort          @ 0x10 data abort
    b       hang                @ 0x14
    b       hang                @ 0x18 interrupt
    b       hang                @ 0x1c fast interrupt

_start:
    @ Turn alignment checking on, the way Linux leaves it.
    mrc     p15, 0, r0, c1, c0, 0
    orr     r0, r0, #2
    mcr     p15, 0, r0, c1, c0, 0

    mov     r0, #'A'
    bl      putc

    sub     sp, sp, #256
    bic     sp, sp, #7          @ eight-byte aligned
    mov     r4, #0
    mov     r5, #0

    strd    r4, r5, [sp, #16]   @ eight-byte aligned
    mov     r0, #'B'
    bl      putc

    add     sp, sp, #4          @ four past a multiple of eight
    strd    r4, r5, [sp, #16]   @ the question

    mov     r0, #'C'
    bl      putc

    @ The control: two past a multiple of eight is not word aligned, and an
    @ alignment check that does not catch this is not a check at all.
    add     sp, sp, #2
    strd    r4, r5, [sp, #16]

    mov     r0, #'D'
    bl      putc
    b       hang

data_abort:
    mov     r6, lr
    mov     r0, #'F'
    bl      putc
    mrc     p15, 0, r0, c5, c0, 0    @ fault status
    bl      puthex
    mrc     p15, 0, r0, c6, c0, 0    @ fault address
    bl      puthex
    b       hang

@ Write r0 as eight hexadecimal digits, most significant first.
puthex:
    mov     r1, #28
1:  mov     r2, r0, lsr r1
    and     r2, r2, #15
    cmp     r2, #10
    addlt   r2, r2, #'0'
    addge   r2, r2, #'a'-10
    push    {r0, r1, lr}
    mov     r0, r2
    bl      putc
    pop     {r0, r1, lr}
    subs    r1, r1, #4
    bpl     1b
    mov     r0, #' '
    b       putc

putc:
    ldr     r1, =UART
    str     r0, [r1]
    bx      lr

hang:
    b       hang

    .section .note.GNU-stack,"",%progbits
