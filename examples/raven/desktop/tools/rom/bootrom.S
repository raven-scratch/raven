@ The boot ROM.
@
@ This is the machine's firmware, and its job is what a boot loader's job always
@ is: bring the board up far enough to be able to say something, show that the
@ machine works, put the guest in memory, and hand it the machine.
@
@ It runs in three acts.
@
@   1. Bring-up. A stack, because the reset value of the stack pointer is zero
@      and the handler below saves registers. The console. A checkerboard in a
@      framebuffer, which the LCD controller is then pointed at and switched on.
@      The timer. The interrupt controller. Interrupts unmasked, and then a spin
@      long enough for the timer to tick a few times -- so that the dots on the
@      console are interrupts arriving and returning, and not a program.
@
@   2. The load. Three blocks are copied out of the flash chip into memory: the
@      kernel, the initramfs and the device tree. The offsets and the
@      destinations are the board file's, handed to the assembler as constants,
@      and the same destinations are what the device tree is built with -- so
@      the ROM and the tree cannot disagree about where anything is.
@
@   3. The handover. Interrupts masked again, because the kernel is about to
@      install its own vectors and a tick arriving in the gap would land in
@      ours. The jump is to a label in the *flash window* rather than here,
@      because the first thing that happens there is clearing the board's remap
@      bit -- which puts SDRAM at address zero and takes this code out from
@      under the program counter. The flash window is where the same bytes are
@      readable afterwards, and that is why a Versatile can do this at all.
@
@ It is assembled by `tools/wsl/05-rom.sh` with the same cross toolchain the
@ guest is built with, and it lands in `images/bootrom.bin`, which
@ `tools/build.mjs` lays into the flash chip. Nothing here is hand-encoded: an
@ ARM instruction written as a number is an ARM instruction nobody can read,
@ and the first version of this ROM got one wrong and executed its own literal
@ pool.

    .arm
    .section .text, "ax"
    .global _start

    .equ UART_BASE,   0x101F1000
    .equ VIC_BASE,    0x10140000
    .equ TIMER_BASE,  0x101E2000
    .equ SYS_BASE,    0x10000000
    .equ CLCD_BASE,   0x10120000
    .equ SMC_BASE,    0x10100000
    .equ FLASH_BASE,  0x34000000

    @ The stack, and the framebuffer the pattern goes in. Both are in SDRAM, and
    @ the framebuffer is high enough that the kernel, the initramfs and the
    @ device tree -- all of which are loaded low -- cannot land on it.
    .equ STACK_TOP,     0x00800000
    .equ IRQ_SAVE,      0x007FF000
    .equ FB_BASE,       0x00D00000

    .equ VIC_INT_ENABLE, 0x010
    .equ VIC_VECT_ADDR,  0x030

    @ SP804 registers, from `timer-sp.h`
    .equ TIMER_LOAD,      0x00
    .equ TIMER_CTRL,      0x08
    .equ TIMER_INTCLR,    0x0C
    .equ TIMER_CTRL_WORD, 0xE2      @ 32-bit, interrupt, periodic, running
    .equ TIMER_PERIOD,    2048

    @ CLCD registers, in the arrangement the Versatile actually has: the control
    @ register at 0x18 and the interrupt enable at 0x1C, which is the PL111
    @ layout and not the PL110 one.
    .equ CLCD_TIM0,   0x00
    .equ CLCD_TIM1,   0x04
    .equ CLCD_TIM2,   0x08
    .equ CLCD_UBAS,   0x10
    .equ CLCD_CNTL,   0x18

    @ SYS_CLCD: the panel identity in bits 12:8 and the two bits that tell the
    @ board's PLD how a sixteen-bit pixel's channels are arranged. Zero is the
    @ Sanyo panel; the value below is mode three, the kernel's own
    @ `SYS_CLCD_MODE_565_B_LSB` -- blue in the low bits and red in the high,
    @ which is the ordinary RGB565 arrangement and the one the checkerboard
    @ above is written in. Mode two is the other way round and would put the
    @ pattern's red primary on the panel's blue channel.
    .equ SYS_CLCD,      0x50
    .equ SYS_CLCD_WORD, 3

    @ A 320 by 240 panel at ten megahertz, written the way the kernel writes it.
    .equ CLCD_TIM0_WORD, (19 << 2) | (5 << 8) | (5 << 16) | (5 << 24)
    .equ CLCD_TIM1_WORD, (239) | (5 << 10) | (5 << 16) | (5 << 24)
    .equ CLCD_TIM2_WORD, (319 << 16)
    .equ CLCD_CNTL_WORD, (1 | 32 | (4 << 1) | 2048)   @ enable, TFT, 16bpp, power

    @ Where the guest lives in the flash and where it goes in memory. These come
    @ from the board file through the assembler's command line, which is why
    @ `05-rom.sh` reads `images/` and passes them: the board file is the one
    @ place the layout is written down.
    .equ KERNEL_OFFSET, FLASH_KERNEL_OFFSET
    .equ KERNEL_DEST,   FLASH_KERNEL_DEST
    .equ KERNEL_BYTES,  FLASH_KERNEL_BYTES
    .equ INITRD_OFFSET, FLASH_INITRD_OFFSET
    .equ INITRD_DEST,   FLASH_INITRD_DEST
    .equ INITRD_BYTES,  FLASH_INITRD_BYTES
    .equ DTB_OFFSET,    FLASH_DTB_OFFSET
    .equ DTB_DEST,      FLASH_DTB_DEST
    .equ DTB_BYTES,     FLASH_DTB_BYTES

_start:
    b   reset               @ 0x00  reset
    b   hang                @ 0x04  undefined instruction
    b   hang                @ 0x08  supervisor call
    b   hang                @ 0x0C  prefetch abort
    b   hang                @ 0x10  data abort
    b   hang                @ 0x14  reserved
    b   irq                 @ 0x18  interrupt
    b   hang                @ 0x1C  fast interrupt

hang:
    b   hang

reset:
    @ Every mode has its own r13, and the handler below runs in IRQ mode --
    @ so the stack it saves into is a *different* register from this one, and
    @ its reset value is zero. Leaving it there is a handler that descends
    @ below address zero on its first save and reads back the zero an
    @ unbacked address reads back as on its restore, which puts a zero in
    @ every register it was asked to preserve. It looks like a working
    @ handler and it silently corrupts its caller: the first version of this
    @ ROM left it there and a register the interrupted code was counting
    @ with came back zero on every tick.
    ldr sp, =STACK_TOP

    @ ---- the console -----------------------------------------------------
    ldr r4, =UART_BASE
    mov r0, #'H'
    str r0, [r4]
    mov r0, #'i'
    str r0, [r4]
    mov r0, #'\n'
    str r0, [r4]

    @ ---- the display -----------------------------------------------------
    @ A checkerboard of eight colours, two panel pixels to a register. The
    @ pattern is written a block at a time with a multiple store, because a
    @ framebuffer is seventy-six thousand pixels and one store each is most of a
    @ minute of wall clock.
    ldr r8, =colours
    ldr r10, =FB_BASE
    mov r11, #0                             @ row
row_loop:
    mov r9, r10
    mov r12, #0                             @ thirty-two pixel block, ten a row
block_loop:
    mov r1, r11, lsr #5
    add r1, r1, r12
    and r1, r1, #7
    ldr r0, [r8, r1, lsl #2]
    orr r0, r0, r0, lsl #16
    mov r4, r0
    mov r5, r0
    mov r6, r0
    mov r7, r0
    stmia r9!, {r4-r7}
    stmia r9!, {r4-r7}
    stmia r9!, {r4-r7}
    stmia r9!, {r4-r7}
    add r12, r12, #1
    cmp r12, #10
    blt block_loop
    add r10, r10, #640
    add r11, r11, #1
    cmp r11, #240
    blt row_loop

    ldr r7, =CLCD_BASE
    ldr r1, =SYS_BASE
    ldr r0, =SYS_CLCD_WORD
    str r0, [r1, #SYS_CLCD]
    ldr r0, =CLCD_TIM0_WORD
    str r0, [r7, #CLCD_TIM0]
    ldr r0, =CLCD_TIM1_WORD
    str r0, [r7, #CLCD_TIM1]
    ldr r0, =CLCD_TIM2_WORD
    str r0, [r7, #CLCD_TIM2]
    ldr r0, =FB_BASE
    str r0, [r7, #CLCD_UBAS]
    ldr r0, =CLCD_CNTL_WORD
    str r0, [r7, #CLCD_CNTL]

    @ ---- the timer and the interrupt -------------------------------------
    ldr r5, =TIMER_BASE
    ldr r1, =TIMER_PERIOD
    str r1, [r5, #TIMER_LOAD]
    ldr r1, =TIMER_CTRL_WORD
    str r1, [r5, #TIMER_CTRL]

    ldr r6, =VIC_BASE
    mov r1, #(1 << 4)                       @ timer block zero is line four
    str r1, [r6, #VIC_INT_ENABLE]

    mrs r0, cpsr
    bic r0, r0, #0x80
    msr cpsr_c, r0

    @ Spin long enough for the timer to tick. Every dot below is an interrupt
    @ that arrived, was answered and returned from.
    ldr r0, =0x4000
wait_ticks:
    subs r0, r0, #1
    bne wait_ticks

    @ Mask interrupts again. The kernel installs its own vectors in a moment and
    @ a tick arriving in the gap would land in ours, on a stack that is about to
    @ stop being a stack.
    mrs r0, cpsr
    orr r0, r0, #0x80
    msr cpsr_c, r0
    ldr r5, =TIMER_BASE
    mov r1, #0
    str r1, [r5, #TIMER_CTRL]

    @ ---- the load --------------------------------------------------------
    ldr r0, =FLASH_BASE + KERNEL_OFFSET
    ldr r1, =KERNEL_DEST
    ldr r2, =KERNEL_BYTES
    bl  copy_block
    ldr r0, =FLASH_BASE + INITRD_OFFSET
    ldr r1, =INITRD_DEST
    ldr r2, =INITRD_BYTES
    bl  copy_block
    ldr r0, =FLASH_BASE + DTB_OFFSET
    ldr r1, =DTB_DEST
    ldr r2, =DTB_BYTES
    bl  copy_block

    @ ---- the handover ----------------------------------------------------
    @ The rest of this runs from the flash window, because the next statement
    @ takes this code away from the address the program counter is using.
    adr r0, launch
    ldr r1, =FLASH_BASE
    add r0, r0, r1
    mov pc, r0

    .ltorg

@ Copy `r2` bytes from `r0` to `r1`, thirty-two at a time. The lengths are
@ rounded up by the build script, so a block that runs a few bytes past the end
@ of its image copies whatever is beside it in the flash and nothing reads it.
copy_block:
    cmp r2, #0
    bxle lr
copy_loop:
    ldmia r0!, {r3-r10}
    stmia r1!, {r3-r10}
    subs r2, r2, #32
    bgt copy_loop
    bx  lr

@ Running from the flash window, with SDRAM still behind the alias.
launch:
    @ Clear the board's remap bit: address zero becomes SDRAM and this code
    @ stops being there. It is still readable at 0x34000000, which is where it
    @ is running from, so the fetch below is unaffected.
    ldr r0, =SMC_BASE
    mov r1, #0
    str r1, [r0]

    @ The ARM boot protocol: r0 is zero, r1 is the machine type -- which the
    @ kernel ignores when r2 names a device tree -- and r2 is the physical
    @ address of the tree. The kernel reads the magic at the front of it and
    @ takes the machine's description from there.
    mov r0, #0
    mov r1, #0
    ldr r2, =DTB_DEST
    ldr pc, =KERNEL_DEST

    .ltorg

@ One tick. Acknowledge the timer, say so, acknowledge the controller, return.
@
@ The order matters and it is the order a real handler uses: the device's status
@ has to be cleared before the controller is acknowledged, or the line comes
@ straight back and the processor never leaves the handler.
@
@ The registers it uses are saved and restored, because an interrupt arrives
@ wherever and whenever it likes.
irq:
    @ The handler saves into a fixed area of memory rather than onto a
    @ stack, and that is deliberate. Every mode has its own r13, so the
    @ stack this handler would use is a register the reset code has to set
    @ -- and a handler that saves into a register that was never
    @ initialised descends below address zero and reads back the zero an
    @ unbacked address reads back as, which silently puts a zero in every
    @ register it was asked to preserve. An area at a known address cannot
    @ be wrong about where it is, and interrupts are masked on entry, so
    @ there is no second handler to collide with it.
    ldr r12, =IRQ_SAVE
    stmia r12, {r0-r8}

    @ Clear the device before acknowledging the controller, or the line
    @ comes straight back and the processor never leaves the handler.
    ldr r5, =TIMER_BASE
    mov r1, #1
    str r1, [r5, #TIMER_INTCLR]

    ldr r4, =UART_BASE
    mov r0, #'.'
    str r0, [r4]

    ldr r6, =VIC_BASE
    str r6, [r6, #VIC_VECT_ADDR]

    ldr r12, =IRQ_SAVE
    ldmia r12, {r0-r8}

    @ Every ARM instruction carries four bits of condition and every
    @ exception returns the same way: the link register holds the address of
    @ the instruction after the one that was interrupted, plus four, and
    @ writing it to the program counter restores the saved status register.
    subs pc, lr, #4
    .align 2
colours:
    .word 0x0000        @ black
    .word 0xF800        @ red
    .word 0x07E0        @ green
    .word 0x001F        @ blue
    .word 0xFFE0        @ yellow
    .word 0x07FF        @ cyan
    .word 0xF81F        @ magenta
    .word 0xFFFF        @ white
