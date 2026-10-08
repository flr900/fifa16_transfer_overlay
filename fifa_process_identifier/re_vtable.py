"""
Ferramenta de análise OFFLINE da imagem despejada (`%TEMP%\fifa16_imagem.bin`), usada em 2026-10-08.
Não toca no jogo. A faixa de código real desta build é RVA 0x39FC000..0x9514000 (seção "`.tls em`").
Ver `_bmad-output/planning-artifacts/integracao-negociacao.md`.
"""
import os, sys, mmap, struct
from capstone import Cs, CS_ARCH_X86, CS_MODE_64
tmp = os.environ['TEMP']
f = open(os.path.join(tmp, 'fifa16_imagem.bin'), 'rb')
mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
BASE = 0x140000000
rva = int(sys.argv[1], 16); n = int(sys.argv[2]) if len(sys.argv) > 2 else 8
md = Cs(CS_ARCH_X86, CS_MODE_64)
def texto(r):
    b = mm[r:r+70]; z = b.find(b'\0'); t = b[:z if z>=0 else 70]
    return t.decode('latin1') if len(t) >= 4 and all(32 <= c < 127 for c in t) else None
for i in range(n):
    v = struct.unpack_from('<Q', mm, rva + 8*i)[0]
    r = v - BASE
    print(f'slot {i}: -> VA 0x{v:X} (RVA 0x{r:X})' + ('' if 0 < r < len(mm) else '  [fora da imagem]'))
    if 0 < r < len(mm) - 64:
        for ins in list(md.disasm(mm[r:r+48], v))[:7]:
            extra = ''
            if ins.mnemonic == 'lea' and 'rip' in ins.op_str:
                import re
                m = re.search(r'rip ([+-]) (0x[0-9a-f]+)', ins.op_str)
                d = int(m.group(2), 16) * (1 if m.group(1)=='+' else -1)
                a = ins.address + ins.size + d - BASE
                t = texto(a) if 0 < a < len(mm) else None
                extra = f'   ; 0x{a:X}' + (f' "{t}"' if t else '')
            print(f'     {ins.address-BASE:08X}  {ins.mnemonic} {ins.op_str}{extra}')
