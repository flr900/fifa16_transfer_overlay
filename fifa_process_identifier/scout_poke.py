"""
Escrita CONFERIDA na memória do FIFA (compare-and-write), para o experimento
do scout nativo. Só escreve se os int32 atuais forem exatamente os esperados;
depois relê para confirmar. Nunca escreve às cegas.

Uso (terminal elevado, jogo aberto):

    python scout_poke.py 0x8D009930 --expect 73885,1048578,27,20260706,-1 \
                                    --write  73885,65535,198,20260706,-1
    python scout_poke.py 0x8D009930 --read 5      # só lê 5 int32

Os endereços mudam a cada sessão do jogo: ache o endereço de novo com
`scout_probe.py capture <rótulo> --seq <registro atual>` antes de escrever.
"""
from __future__ import annotations

import argparse
import ctypes
import struct
import sys
from ctypes import wintypes

import memory
import process

sys.stdout.reconfigure(encoding="utf-8")

WriteProcessMemory = memory.kernel32.WriteProcessMemory
WriteProcessMemory.argtypes = [
    wintypes.HANDLE,
    ctypes.c_void_p,
    ctypes.c_void_p,
    ctypes.c_size_t,
    ctypes.POINTER(ctypes.c_size_t),
]
WriteProcessMemory.restype = wintypes.BOOL


def read_ints(handle, addr: int, n: int) -> list[int]:
    buf = ctypes.create_string_buffer(4 * n)
    got = ctypes.c_size_t(0)
    ok = memory.ReadProcessMemory(handle, ctypes.c_void_p(addr), buf, 4 * n, ctypes.byref(got))
    if not ok or got.value != 4 * n:
        sys.exit(f"[ERRO] não consegui ler {n} int32 em 0x{addr:X}")
    return list(struct.unpack(f"<{n}i", buf.raw))


def parse(csv: str) -> list[int]:
    return [int(x) for x in csv.split(",")]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("addr", help="endereço (hex), ex.: 0x8D009930")
    ap.add_argument("--expect", help="int32 que DEVEM estar lá agora (vírgulas)")
    ap.add_argument("--write", help="int32 novos (mesma quantidade do --expect)")
    ap.add_argument("--read", type=int, help="só ler N int32")
    args = ap.parse_args()
    addr = int(args.addr, 16)

    pid = process.find_process_by_name("FIFA16.exe")
    if pid is None:
        sys.exit("[ERRO] FIFA16.exe não está rodando.")

    if args.read:
        handle = process.open_process(pid)
        print([int(v) for v in read_ints(handle, addr, args.read)])
        return

    if not (args.expect and args.write):
        sys.exit("[ERRO] informe --expect e --write (ou --read N)")
    old, new = parse(args.expect), parse(args.write)
    if len(old) != len(new):
        sys.exit("[ERRO] --expect e --write precisam ter o mesmo tamanho")

    handle = process.open_process_write(pid)
    atual = read_ints(handle, addr, len(old))
    print("atual   :", atual)
    if atual != old:
        sys.exit("[RECUSADO] o conteúdo atual não é o esperado; nada foi escrito")

    payload = struct.pack(f"<{len(new)}i", *new)
    wrote = ctypes.c_size_t(0)
    ok = WriteProcessMemory(handle, ctypes.c_void_p(addr), payload, len(payload), ctypes.byref(wrote))
    if not ok or wrote.value != len(payload):
        sys.exit(f"[ERRO] WriteProcessMemory falhou (erro {ctypes.get_last_error()})")
    depois = read_ints(handle, addr, len(new))
    print("depois  :", depois)
    print("OK" if depois == new else "[ALERTA] a releitura não bateu (o jogo pode ter sobrescrito)")


if __name__ == "__main__":
    main()
