"""
Análise OFFLINE da imagem do fifa16.exe despejada pelo overlay
(`%TEMP%\\fifa16_imagem.bin`, ver `fifa_overlay/src/despejo.rs`). Não toca no jogo.

Objetivo: achar no código quem dispara eventos de tela como
`EnterTransferOfferActionPopup` (a função que escreve o nome do evento na
tabela de eventos e a que a chama), para saber se dá para abrir a tela direto.

Uso (sem dependências externas):

    python analisar_imagem.py secoes   <imagem.bin>
    python analisar_imagem.py strings  <imagem.bin> <texto> [--limite 20]
    python analisar_imagem.py xrefs    <imagem.bin> <texto> [--limite 20]
    python analisar_imagem.py chamadores <imagem.bin> <rva_da_funcao> [--limite 40]
    python analisar_imagem.py dump     <imagem.bin> <rva> [bytes]

`imagem.bin` pode ser o despejo da memória (o deslocamento é o RVA) ou o .exe
em disco (as seções são mapeadas pelo cabeçalho; `--disco` força).

Referências de string no x64 são `lea reg, [rip+disp32]` (48/4C 8D /r com
mod=00 rm=101): o alvo é o RVA da instrução seguinte + disp32.
"""
from __future__ import annotations

import argparse
import math
import re
import struct
import sys
from dataclasses import dataclass

sys.stdout.reconfigure(encoding="utf-8")

IMAGE_SCN_MEM_EXECUTE = 0x20000000
IMAGE_SCN_CNT_CODE = 0x00000020
LEA_RIP = re.compile(rb"[\x48\x4c]\x8d[\x05\x0d\x15\x1d\x25\x2d\x35\x3d]", re.S)
CALL_REL32 = re.compile(rb"\xe8", re.S)


@dataclass
class Secao:
    nome: str
    rva: int
    vsize: int
    raw_ptr: int
    raw_size: int
    flags: int

    @property
    def executavel(self) -> bool:
        return bool(self.flags & (IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_CNT_CODE))


class Imagem:
    def __init__(self, dados: bytes, disco: bool | None = None):
        self.dados = dados
        if dados[:2] != b"MZ":
            raise SystemExit("[ERRO] não parece um executável (sem 'MZ' no começo).")
        e_lfanew = struct.unpack_from("<I", dados, 0x3C)[0]
        if dados[e_lfanew : e_lfanew + 4] != b"PE\0\0":
            raise SystemExit("[ERRO] cabeçalho PE inválido.")
        n_secoes, = struct.unpack_from("<H", dados, e_lfanew + 6)
        tam_opt, = struct.unpack_from("<H", dados, e_lfanew + 20)
        opt = e_lfanew + 24
        self.magic = struct.unpack_from("<H", dados, opt)[0]
        self.base_preferida = struct.unpack_from("<Q", dados, opt + 24)[0]
        self.tamanho_da_imagem = struct.unpack_from("<I", dados, opt + 56)[0]
        self.entrada = struct.unpack_from("<I", dados, opt + 16)[0]
        sec = opt + tam_opt
        self.secoes: list[Secao] = []
        for i in range(n_secoes):
            nome, vsize, rva, raw_size, raw_ptr = struct.unpack_from("<8sIIII", dados, sec + i * 40)
            flags = struct.unpack_from("<I", dados, sec + i * 40 + 36)[0]
            self.secoes.append(Secao(nome.rstrip(b"\0").decode("latin1"), rva, vsize, raw_ptr, raw_size, flags))
        # despejo da memória: o arquivo tem o tamanho da imagem e o offset é o RVA
        self.disco = (len(dados) < self.tamanho_da_imagem) if disco is None else disco

    def rva_para_offset(self, rva: int) -> int | None:
        if not self.disco:
            return rva if rva < len(self.dados) else None
        for s in self.secoes:
            if s.rva <= rva < s.rva + max(s.vsize, s.raw_size):
                off = s.raw_ptr + (rva - s.rva)
                return off if off < len(self.dados) else None
        return rva if rva < 0x400 else None

    def offset_para_rva(self, off: int) -> int | None:
        if not self.disco:
            return off
        for s in self.secoes:
            if s.raw_ptr <= off < s.raw_ptr + s.raw_size:
                return s.rva + (off - s.raw_ptr)
        return off if off < 0x400 else None

    def fatia(self, s: Secao) -> bytes:
        if not self.disco:
            return self.dados[s.rva : s.rva + max(s.vsize, 1)]
        return self.dados[s.raw_ptr : s.raw_ptr + s.raw_size]

    def ler(self, rva: int, n: int) -> bytes:
        off = self.rva_para_offset(rva)
        return b"" if off is None else self.dados[off : off + n]


def entropia(b: bytes) -> float:
    if not b:
        return 0.0
    contagem = [0] * 256
    for x in b[:2_000_000]:
        contagem[x] += 1
    n = min(len(b), 2_000_000)
    return -sum(c / n * math.log2(c / n) for c in contagem if c)


def parece_codigo(b: bytes) -> float:
    """Fração aproximada de bytes típicos de código x64 numa amostra (REX 0x48,
    call/jmp E8/E9, ret C3, int3 CC, mov 8B/89, lea 8D). Código de verdade
    passa de ~0,25; dados criptografados ficam perto de 0,06."""
    amostra = b[: min(len(b), 400_000)]
    if not amostra:
        return 0.0
    comuns = sum(amostra.count(bytes([c])) for c in (0x48, 0xE8, 0xE9, 0xC3, 0xCC, 0x8B, 0x89, 0x8D, 0x0F, 0x83))
    return comuns / len(amostra)


def cmd_secoes(im: Imagem, _a) -> None:
    modo = "disco" if im.disco else "memória (offset = RVA)"
    print(f"Imagem: {len(im.dados):,} bytes, modo {modo}; base preferida 0x{im.base_preferida:X}; "
          f"SizeOfImage 0x{im.tamanho_da_imagem:X}; entrada RVA 0x{im.entrada:X}")
    print(f"{'seção':<10}{'RVA':>10}{'tamanho':>12}  {'exec':<5}{'entropia':>9}{'%código':>9}")
    for s in im.secoes:
        d = im.fatia(s)
        print(f"{s.nome:<10}{s.rva:>#10x}{s.vsize:>#12x}  {'sim' if s.executavel else 'não':<5}"
              f"{entropia(d):>9.2f}{parece_codigo(d) * 100:>8.1f}%")
    print("\nCódigo de verdade: entropia ~5.5-6.5 e %código > 25. Entropia > 7.5 e %código < 10 = ainda embaralhado.")


def agulhas(texto: str) -> list[tuple[bytes, str]]:
    return [(texto.encode("latin-1", "replace"), "ascii"), (texto.encode("utf-16-le"), "utf16")]


def achar_strings(im: Imagem, texto: str, limite: int) -> list[tuple[int, str]]:
    achados: list[tuple[int, str]] = []
    for agulha, enc in agulhas(texto):
        pos = im.dados.find(agulha)
        while pos != -1 and len(achados) < limite * 4:
            rva = im.offset_para_rva(pos)
            if rva is not None:
                achados.append((rva, enc))
            pos = im.dados.find(agulha, pos + 1)
    return achados


def contexto_texto(im: Imagem, rva: int, enc: str) -> str:
    b = im.ler(rva, 120 if enc == "ascii" else 240)
    if enc == "utf16":
        b = b[::2]
    return re.split(rb"\x00", b)[0].decode("latin-1", "replace")


def cmd_strings(im: Imagem, a) -> None:
    for rva, enc in achar_strings(im, a.texto, a.limite)[: a.limite]:
        print(f"RVA 0x{rva:X} ({enc}): {contexto_texto(im, rva, enc)!r}")


def inicio_de_funcao(codigo: bytes, pos: int) -> int:
    """Heurística: o começo da função é logo depois do último bloco de
    preenchimento (CC CC...) alinhado em 16 antes de `pos`."""
    i = pos
    limite = max(0, pos - 6000)
    while i > limite:
        if codigo[i - 1] == 0xCC and codigo[i] != 0xCC and i % 16 == 0:
            return i
        i -= 1
    return pos


def refs_a(im: Imagem, alvos: set[int], limite: int) -> list[tuple[int, int, Secao]]:
    """(rva da instrução, alvo, seção) de cada `lea reg,[rip+disp]` que cai em `alvos`."""
    achados = []
    for s in im.secoes:
        if not s.executavel:
            continue
        codigo = im.fatia(s)
        for m in LEA_RIP.finditer(codigo):
            p = m.start()
            if p + 7 > len(codigo):
                continue
            disp = struct.unpack_from("<i", codigo, p + 3)[0]
            alvo = s.rva + p + 7 + disp
            if alvo in alvos:
                achados.append((s.rva + p, alvo, s))
                if len(achados) >= limite:
                    return achados
    return achados


def hexdump(b: bytes, rva: int) -> str:
    linhas = []
    for i in range(0, len(b), 16):
        parte = b[i : i + 16]
        linhas.append(f"  {rva + i:08X}  {' '.join(f'{x:02X}' for x in parte):<48} {''.join(chr(x) if 32 <= x < 127 else '.' for x in parte)}")
    return "\n".join(linhas)


def cmd_xrefs(im: Imagem, a) -> None:
    ocorrencias = achar_strings(im, a.texto, a.limite)
    if not ocorrencias:
        print("Nenhuma ocorrência da string.")
        return
    alvos = {rva for rva, _ in ocorrencias}
    print(f"{len(ocorrencias)} ocorrência(s) da string; procurando `lea reg,[rip+x]` que apontam para elas...")
    refs = refs_a(im, alvos, a.limite)
    if not refs:
        print("Nenhuma referência direta (a string pode ser montada de outro jeito, ou o código ainda está embaralhado).")
        return
    for rva_instr, alvo, s in refs:
        codigo = im.fatia(s)
        pos = rva_instr - s.rva
        ini = inicio_de_funcao(codigo, pos)
        print(f"\nREF em 0x{rva_instr:X} (seção {s.nome}) -> string 0x{alvo:X}; função provável começa em 0x{s.rva + ini:X} (+0x{pos - ini:X})")
        print(hexdump(codigo[max(0, pos - 24) : pos + 40], s.rva + max(0, pos - 24)))


def cmd_chamadores(im: Imagem, a) -> None:
    alvo = int(a.rva, 16)
    achados = []
    for s in im.secoes:
        if not s.executavel:
            continue
        codigo = im.fatia(s)
        for m in CALL_REL32.finditer(codigo):
            p = m.start()
            if p + 5 > len(codigo):
                continue
            rel = struct.unpack_from("<i", codigo, p + 1)[0]
            if s.rva + p + 5 + rel == alvo:
                achados.append((s.rva + p, s, p))
                if len(achados) >= a.limite:
                    break
    print(f"{len(achados)} `call` direto(s) para 0x{alvo:X}:")
    for rva, s, p in achados:
        ini = inicio_de_funcao(im.fatia(s), p)
        print(f"  call em 0x{rva:X}  (função que chama: ~0x{s.rva + ini:X})")


def cmd_dump(im: Imagem, a) -> None:
    rva = int(a.rva, 16)
    print(hexdump(im.ler(rva, a.bytes), rva))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    for nome in ("secoes", "strings", "xrefs", "chamadores", "dump"):
        p = sub.add_parser(nome)
        p.add_argument("imagem")
        p.add_argument("--disco", action="store_true", help="a imagem é o .exe em disco")
        if nome in ("strings", "xrefs"):
            p.add_argument("texto")
        if nome in ("chamadores", "dump"):
            p.add_argument("rva", help="RVA em hexadecimal (ex.: 1A2B30)")
        if nome == "dump":
            p.add_argument("bytes", nargs="?", type=int, default=128)
        p.add_argument("--limite", type=int, default=20)
    a = ap.parse_args()
    with open(a.imagem, "rb") as f:
        dados = f.read()
    im = Imagem(dados, True if a.disco else None)
    {"secoes": cmd_secoes, "strings": cmd_strings, "xrefs": cmd_xrefs, "chamadores": cmd_chamadores, "dump": cmd_dump}[a.cmd](im, a)


if __name__ == "__main__":
    main()
