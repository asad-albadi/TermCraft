
app/target/agent-work/original-runtime/original/MobaXterm_Personal_26.6.exe:     file format pei-i386


Disassembly of section CODE:

009c4434 <CODE+0x5c3434>:
  9c4434:	55                   	push   ebp
  9c4435:	8b ec                	mov    ebp,esp
  9c4437:	83 c4 f8             	add    esp,0xfffffff8
  9c443a:	53                   	push   ebx
  9c443b:	56                   	push   esi
  9c443c:	57                   	push   edi
  9c443d:	89 55 f8             	mov    DWORD PTR [ebp-0x8],edx
  9c4440:	89 45 fc             	mov    DWORD PTR [ebp-0x4],eax
  9c4443:	6a 00                	push   0x0
  9c4445:	6a 01                	push   0x1
  9c4447:	a1 68 78 be 00       	mov    eax,ds:0xbe7868
  9c444c:	8b 00                	mov    eax,DWORD PTR [eax]
  9c444e:	33 c9                	xor    ecx,ecx
  9c4450:	ba fc 44 9c 00       	mov    edx,0x9c44fc
  9c4455:	e8 82 c6 0a 00       	call   0xa70adc
  9c445a:	8b 55 f8             	mov    edx,DWORD PTR [ebp-0x8]
  9c445d:	8b 45 fc             	mov    eax,DWORD PTR [ebp-0x4]
  9c4460:	e8 0f 78 00 00       	call   0x9cbc74
  9c4465:	8b 45 fc             	mov    eax,DWORD PTR [ebp-0x4]
  9c4468:	8b 80 98 06 00 00    	mov    eax,DWORD PTR [eax+0x698]
  9c446e:	80 b8 70 01 00 00 00 	cmp    BYTE PTR [eax+0x170],0x0
  9c4475:	75 1d                	jne    0x9c4494
  9c4477:	68 40 10 04 00       	push   0x41040
  9c447c:	b9 20 45 9c 00       	mov    ecx,0x9c4520
  9c4481:	ba 2c 45 9c 00       	mov    edx,0x9c452c
  9c4486:	a1 28 72 be 00       	mov    eax,ds:0xbe7228
  9c448b:	8b 00                	mov    eax,DWORD PTR [eax]
  9c448d:	e8 72 e1 ab ff       	call   0x482604
  9c4492:	eb 58                	jmp    0x9c44ec
  9c4494:	33 c0                	xor    eax,eax
  9c4496:	55                   	push   ebp
  9c4497:	68 c7 44 9c 00       	push   0x9c44c7
  9c449c:	64 ff 30             	push   DWORD PTR fs:[eax]
  9c449f:	64 89 20             	mov    DWORD PTR fs:[eax],esp
  9c44a2:	8b 45 fc             	mov    eax,DWORD PTR [ebp-0x4]
  9c44a5:	8b 80 74 06 00 00    	mov    eax,DWORD PTR [eax+0x674]
  9c44ab:	33 d2                	xor    edx,edx
  9c44ad:	e8 6a 4e ac ff       	call   0x48931c
  9c44b2:	8b 45 fc             	mov    eax,DWORD PTR [ebp-0x4]
  9c44b5:	8b 10                	mov    edx,DWORD PTR [eax]
  9c44b7:	ff 92 80 00 00 00    	call   DWORD PTR [edx+0x80]
  9c44bd:	33 c0                	xor    eax,eax
  9c44bf:	5a                   	pop    edx
  9c44c0:	59                   	pop    ecx
  9c44c1:	59                   	pop    ecx
  9c44c2:	64 89 10             	mov    DWORD PTR fs:[eax],edx
  9c44c5:	eb 0a                	jmp    0x9c44d1
  9c44c7:	e9 74 02 a4 ff       	jmp    0x404740
  9c44cc:	e8 d7 05 a4 ff       	call   0x404aa8
  9c44d1:	8b 45 fc             	mov    eax,DWORD PTR [ebp-0x4]
  9c44d4:	8b 80 a4 04 00 00    	mov    eax,DWORD PTR [eax+0x4a4]
  9c44da:	c7 40 0c 40 1f 00 00 	mov    DWORD PTR [eax+0xc],0x1f40
  9c44e1:	8b 55 f8             	mov    edx,DWORD PTR [ebp-0x8]
  9c44e4:	8b 45 fc             	mov    eax,DWORD PTR [ebp-0x4]
  9c44e7:	e8 f4 e7 ff ff       	call   0x9c2ce0
  9c44ec:	5f                   	pop    edi
  9c44ed:	5e                   	pop    esi
  9c44ee:	5b                   	pop    ebx
  9c44ef:	59                   	pop    ecx
  9c44f0:	59                   	pop    ecx
  9c44f1:	5d                   	pop    ebp
  9c44f2:	c3                   	ret
