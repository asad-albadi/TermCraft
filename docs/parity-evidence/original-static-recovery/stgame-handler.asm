
app/target/agent-work/original-runtime/original/MobaXterm_Personal_26.6.exe:     file format pei-i386


Disassembly of section CODE:

009e68c0 <CODE+0x5e58c0>:
  9e68c0:	55                   	push   ebp
  9e68c1:	8b ec                	mov    ebp,esp
  9e68c3:	33 c9                	xor    ecx,ecx
  9e68c5:	51                   	push   ecx
  9e68c6:	51                   	push   ecx
  9e68c7:	51                   	push   ecx
  9e68c8:	51                   	push   ecx
  9e68c9:	53                   	push   ebx
  9e68ca:	56                   	push   esi
  9e68cb:	8b da                	mov    ebx,edx
  9e68cd:	8b f0                	mov    esi,eax
  9e68cf:	33 c0                	xor    eax,eax
  9e68d1:	55                   	push   ebp
  9e68d2:	68 a9 69 9e 00       	push   0x9e69a9
  9e68d7:	64 ff 30             	push   DWORD PTR fs:[eax]
  9e68da:	64 89 20             	mov    DWORD PTR fs:[eax],esp
  9e68dd:	6a 00                	push   0x0
  9e68df:	6a 01                	push   0x1
  9e68e1:	a1 68 78 be 00       	mov    eax,ds:0xbe7868
  9e68e6:	8b 00                	mov    eax,DWORD PTR [eax]
  9e68e8:	33 c9                	xor    ecx,ecx
  9e68ea:	ba c0 69 9e 00       	mov    edx,0x9e69c0
  9e68ef:	e8 e8 a1 08 00       	call   0xa70adc
  9e68f4:	8b d3                	mov    edx,ebx
  9e68f6:	8b c6                	mov    eax,esi
  9e68f8:	e8 77 53 fe ff       	call   0x9cbc74
  9e68fd:	8d 45 f8             	lea    eax,[ebp-0x8]
  9e6900:	50                   	push   eax
  9e6901:	b1 01                	mov    cl,0x1
  9e6903:	ba e4 69 9e 00       	mov    edx,0x9e69e4
  9e6908:	8b c6                	mov    eax,esi
  9e690a:	e8 b9 39 f7 ff       	call   0x95a2c8
  9e690f:	8b c3                	mov    eax,ebx
  9e6911:	8b 15 54 52 49 00    	mov    edx,DWORD PTR ds:0x495254
  9e6917:	e8 c8 da a1 ff       	call   0x4043e4
  9e691c:	84 c0                	test   al,al
  9e691e:	74 0d                	je     0x9e692d
  9e6920:	8d 45 fc             	lea    eax,[ebp-0x4]
  9e6923:	8b 53 58             	mov    edx,DWORD PTR [ebx+0x58]
  9e6926:	e8 45 e8 a1 ff       	call   0x405170
  9e692b:	eb 1f                	jmp    0x9e694c
  9e692d:	8b c3                	mov    eax,ebx
  9e692f:	8b 15 c0 82 ab 00    	mov    edx,DWORD PTR ds:0xab82c0
  9e6935:	e8 aa da a1 ff       	call   0x4043e4
  9e693a:	84 c0                	test   al,al
  9e693c:	74 0e                	je     0x9e694c
  9e693e:	8d 45 fc             	lea    eax,[ebp-0x4]
  9e6941:	8b 93 80 00 00 00    	mov    edx,DWORD PTR [ebx+0x80]
  9e6947:	e8 24 e8 a1 ff       	call   0x405170
  9e694c:	6a 05                	push   0x5
  9e694e:	68 00 6a 9e 00       	push   0x9e6a00
  9e6953:	8d 55 f0             	lea    edx,[ebp-0x10]
  9e6956:	a1 68 78 be 00       	mov    eax,ds:0xbe7868
  9e695b:	8b 00                	mov    eax,DWORD PTR [eax]
  9e695d:	8b 80 fc 0e 00 00    	mov    eax,DWORD PTR [eax+0xefc]
  9e6963:	e8 34 c1 a2 ff       	call   0x412a9c
  9e6968:	ff 75 f0             	push   DWORD PTR [ebp-0x10]
  9e696b:	68 0c 6a 9e 00       	push   0x9e6a0c
  9e6970:	ff 75 fc             	push   DWORD PTR [ebp-0x4]
  9e6973:	8d 45 f4             	lea    eax,[ebp-0xc]
  9e6976:	ba 04 00 00 00       	mov    edx,0x4
  9e697b:	e8 04 eb a1 ff       	call   0x405484
  9e6980:	8b 45 f4             	mov    eax,DWORD PTR [ebp-0xc]
  9e6983:	e8 3c ec a1 ff       	call   0x4055c4
  9e6988:	50                   	push   eax
  9e6989:	e8 be 13 a2 ff       	call   0x407d4c
  9e698e:	33 c0                	xor    eax,eax
  9e6990:	5a                   	pop    edx
  9e6991:	59                   	pop    ecx
  9e6992:	59                   	pop    ecx
  9e6993:	64 89 10             	mov    DWORD PTR fs:[eax],edx
  9e6996:	68 b0 69 9e 00       	push   0x9e69b0
  9e699b:	8d 45 f0             	lea    eax,[ebp-0x10]
  9e699e:	ba 04 00 00 00       	mov    edx,0x4
  9e69a3:	e8 54 e7 a1 ff       	call   0x4050fc
  9e69a8:	c3                   	ret
  9e69a9:	e9 46 e0 a1 ff       	jmp    0x4049f4
  9e69ae:	eb eb                	jmp    0x9e699b
  9e69b0:	5e                   	pop    esi
  9e69b1:	5b                   	pop    ebx
  9e69b2:	8b e5                	mov    esp,ebp
  9e69b4:	5d                   	pop    ebp
  9e69b5:	c3                   	ret
