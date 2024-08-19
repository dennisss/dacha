Compile using:

```
RUSTFLAGS="-C opt-level=s -C panic=abort" cargo build --package avr -Z build-std=core --target pkg/peripherals/avr/targets/attiny85.json

scp -i ~/.ssh/id_cluster target/attiny85/debug/avr.elf cluster-user@10.1.1.3:~/
```


A minimal AVR program that just runs an infinite loop:

```
$ avr-objdump -D target/attiny85/debug/avr.elf

Disassembly of section .text:

00000000 <__vectors>:
   0:	0e c0       	rjmp	.+28     	; 0x1e <__ctors_end>
   2:	15 c0       	rjmp	.+42     	; 0x2e <__bad_interrupt>
   4:	14 c0       	rjmp	.+40     	; 0x2e <__bad_interrupt>
   6:	13 c0       	rjmp	.+38     	; 0x2e <__bad_interrupt>
   8:	12 c0       	rjmp	.+36     	; 0x2e <__bad_interrupt>
   a:	11 c0       	rjmp	.+34     	; 0x2e <__bad_interrupt>
   c:	10 c0       	rjmp	.+32     	; 0x2e <__bad_interrupt>
   e:	0f c0       	rjmp	.+30     	; 0x2e <__bad_interrupt>
  10:	0e c0       	rjmp	.+28     	; 0x2e <__bad_interrupt>
  12:	0d c0       	rjmp	.+26     	; 0x2e <__bad_interrupt>
  14:	0c c0       	rjmp	.+24     	; 0x2e <__bad_interrupt>
  16:	0b c0       	rjmp	.+22     	; 0x2e <__bad_interrupt>
  18:	0a c0       	rjmp	.+20     	; 0x2e <__bad_interrupt>
  1a:	09 c0       	rjmp	.+18     	; 0x2e <__bad_interrupt>
  1c:	08 c0       	rjmp	.+16     	; 0x2e <__bad_interrupt>

0000001e <__ctors_end>:
  1e:	11 24       	eor	r1, r1
  20:	1f be       	out	0x3f, r1	; 63
  22:	cf e5       	ldi	r28, 0x5F	; 95
  24:	d2 e0       	ldi	r29, 0x02	; 2
  26:	de bf       	out	0x3e, r29	; 62
  28:	cd bf       	out	0x3d, r28	; 61
  2a:	02 d0       	rcall	.+4      	; 0x30 <main>
  2c:	03 c0       	rjmp	.+6      	; 0x34 <_exit>

0000002e <__bad_interrupt>:
  2e:	e8 cf       	rjmp	.-48     	; 0x0 <__vectors>

00000030 <main>:
  30:	00 00       	nop
  32:	fe cf       	rjmp	.-4      	; 0x30 <main>

00000034 <_exit>:
  34:	f8 94       	cli

00000036 <__stop_program>:
  36:	ff cf       	rjmp	.-2      	; 0x36 <__stop_program>
```


