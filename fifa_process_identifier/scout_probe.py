"""
Sonda do experimento "scout nativo" (GTN, Lista de Escolhidos, e-mails).
SOMENTE LEITURA: nunca escreve na memória do jogo.

A ideia é a mesma do `transferbudget`: tirar uma captura da memória em cada
etapa de um roteiro no jogo e comparar. Cada captura guarda:

- onde estão, na memória PRIVATE legível, os int32 pedidos com `--int`
  (ex.: o playerid do jogador que vai para a lista; a data de retorno de
  uma missão GTN), com 64 bytes de contexto em volta de cada ocorrência;
- onde estão as strings de e-mail/telas do jogo (`CM_Email_*`, `CM_View_*`),
  em ASCII e em UTF-16, para achar a caixa de entrada viva.

Uso (jogo aberto, em Modo Carreira):

    python scout_probe.py capture A --int 70571
    ... faz a ação no jogo ...
    python scout_probe.py capture B --int 70571
    python scout_probe.py diff A B

    # telas do jogo (negociação): procura TEXTOS da interface na memória
    python scout_probe.py capture hub --str "contrato" --str "empréstimo"

    python scout_probe.py list                # capturas salvas
    python scout_probe.py selftest            # testa a sonda em si mesma

Saída das capturas: `probe_runs/<rótulo>.json` (fora do git).
"""
from __future__ import annotations

import argparse
import ctypes
import json
import os
import re
import struct
import sys
import time
from pathlib import Path

import memory
import process

sys.stdout.reconfigure(encoding="utf-8")

RUNS_DIR = Path(__file__).parent / "probe_runs"
CONTEXT = 64  # bytes de cada lado da ocorrência
MAX_HITS_PER_VALUE = 4000
MAX_STRINGS = 4000
MAX_STRINGS_LIVRES = 20000
# Em volta de um texto procurado com --str: letras, números e pontuação
# (Latin-1), para mostrar a frase inteira e não só o pedaço.
CONTEXTO_TEXTO = 40
PROCESS_NAME = "FIFA16.exe"

READABLE = {0x02, 0x04, 0x08, 0x20, 0x40, 0x80}  # RO, RW, WC, X, XRW, XWC
STR_ASCII = re.compile(rb"CM_(?:Email|View|Scout|GTN|Shortlist)[A-Za-z0-9_]{2,60}")
STR_UTF16 = re.compile(
    rb"(?:C\x00M\x00_\x00(?:E\x00m\x00a\x00i\x00l\x00|V\x00i\x00e\x00w\x00)(?:[A-Za-z0-9_]\x00){2,60})"
)


def scan_regions(handle, max_mb: int):
    """Regiões PRIVATE+COMMIT legíveis (qualquer proteção de leitura)."""
    for r in memory.enumerate_regions(handle):
        if r.state != memory.MEM_COMMIT or r.type != memory.MEM_PRIVATE:
            continue
        if r.protect & memory.PAGE_GUARD or r.protect not in READABLE:
            continue
        if r.size > max_mb * 1024 * 1024:
            continue
        yield r


def padroes_de_texto(textos: list[str]) -> list[tuple[re.Pattern, str]]:
    """Um padrão ASCII/Latin-1 e um UTF-16LE por texto (sem diferenciar
    maiúsculas de minúsculas), com a frase em volta."""
    padroes = []
    for texto in textos:
        latin = re.escape(texto.encode("latin-1", "replace"))
        livre = b"[\\x20-\\x7e\\xa0-\\xff]{0,%d}" % CONTEXTO_TEXTO
        padroes.append((re.compile(livre + latin + livre, re.I), "ascii"))
        utf16 = b"".join(re.escape(c.encode("utf-16-le")) for c in texto)
        livre16 = b"(?:[\\x20-\\x7e\\xa0-\\xff]\\x00){0,%d}" % CONTEXTO_TEXTO
        padroes.append((re.compile(livre16 + utf16 + livre16, re.I), "utf16"))
    return padroes


def capture(pid: int, label: str, ints: list[int], max_mb: int, seqs: list[list[int]] | None = None,
            textos: list[str] | None = None) -> Path:
    handle = process.open_process(pid)
    started = time.time()
    seqs = seqs or []
    needles = {struct.pack("<i", v): str(v) for v in ints}
    for q in seqs:
        needles[struct.pack(f"<{len(q)}i", *q)] = "seq:" + ",".join(map(str, q))
    pattern = (
        re.compile(b"(?=(" + b"|".join(re.escape(n) for n in needles) + b"))", re.S)
        if needles
        else None
    )
    hits: dict[str, list[dict]] = {k: [] for k in needles.values()}
    strings: list[dict] = []
    livres = padroes_de_texto(textos or [])
    achados_livres = 0
    regions = bytes_read = 0

    for region in scan_regions(handle, max_mb):
        raw = memory.read_region(handle, region)
        if raw is None:
            continue
        regions += 1
        bytes_read += len(raw)

        if pattern is not None:
            for m in pattern.finditer(raw):
                pos = m.start()
                if pos % 4:
                    continue
                bucket = hits[needles[m.group(1)]]
                if len(bucket) >= MAX_HITS_PER_VALUE:
                    continue
                lo = max(0, pos - CONTEXT)
                bucket.append(
                    {
                        "addr": region.base + pos,
                        "region": region.base,
                        "off": pos,
                        "ctx_off": pos - lo,
                        "ctx": raw[lo : pos + CONTEXT].hex(),
                    }
                )

        for rx, enc in livres:
            if achados_livres >= MAX_STRINGS_LIVRES:
                break
            for m in rx.finditer(raw):
                texto = m.group(0)
                if enc == "utf16":
                    texto = texto[::2]
                strings.append(
                    {
                        "addr": region.base + m.start(),
                        "region": region.base,
                        "enc": enc,
                        "text": "T:" + texto.decode("latin-1", "replace").strip(),
                    }
                )
                achados_livres += 1
                if achados_livres >= MAX_STRINGS_LIVRES:
                    break

        if len(strings) < MAX_STRINGS + achados_livres:
            for rx, enc in ((STR_ASCII, "ascii"), (STR_UTF16, "utf16")):
                for m in rx.finditer(raw):
                    text = m.group(0)
                    if enc == "utf16":
                        text = text[::2]
                    strings.append(
                        {
                            "addr": region.base + m.start(),
                            "region": region.base,
                            "enc": enc,
                            "text": text.decode("ascii", "replace"),
                        }
                    )
                    if len(strings) >= MAX_STRINGS + achados_livres:
                        break

    memory.kernel32.CloseHandle(handle)
    out = {
        "label": label,
        "pid": pid,
        "when": time.strftime("%Y-%m-%d %H:%M:%S"),
        "seconds": round(time.time() - started, 1),
        "regions": regions,
        "mb": round(bytes_read / 1e6),
        "ints": ints + ["seq:" + ",".join(map(str, q)) for q in seqs],
        "hits": hits,
        "strings": strings,
    }
    RUNS_DIR.mkdir(exist_ok=True)
    path = RUNS_DIR / f"{label}.json"
    path.write_text(json.dumps(out), encoding="utf-8")
    print(
        f"[{label}] {regions} regiões, {out['mb']} MB em {out['seconds']} s -> {path.name}"
    )
    for k in hits:
        print(f"  {k}: {len(hits[k])} ocorrências")
    kinds: dict[str, int] = {}
    for s in strings:
        kinds[s["text"]] = kinds.get(s["text"], 0) + 1
    print(f"  strings CM_*: {len(strings) - achados_livres} ocorrências")
    if textos:
        livres_vistos = sorted({t for t in kinds if t.startswith("T:")})
        print(f"  textos procurados: {achados_livres} ocorrências, {len(livres_vistos)} frases distintas")
        for t in livres_vistos[:40]:
            print(f"    {kinds[t]}x {t[2:]!r}")
    return path


def load(label: str) -> dict:
    return json.loads((RUNS_DIR / f"{label}.json").read_text(encoding="utf-8"))


def slots(ctx_hex: str, ctx_off: int) -> str:
    """Contexto como int32 (4 antes ... [valor] ... 4 depois). Plausíveis
    como playerid (1..300000) entram sem marca; os demais, entre colchetes
    só se forem pequenos (flags/contadores)."""
    raw = bytes.fromhex(ctx_hex)
    base = ctx_off - (ctx_off // 4) * 4
    out = []
    for i in range(base, len(raw) - 3, 4):
        v = struct.unpack_from("<i", raw, i)[0]
        mark = "*" if i == ctx_off else " "
        out.append(f"{mark}{v}")
    return " ".join(out)


def plausible_ids(ctx_hex: str, ctx_off: int) -> int:
    raw = bytes.fromhex(ctx_hex)
    base = ctx_off % 4
    n = 0
    for i in range(base, len(raw) - 3, 4):
        if i == ctx_off:
            continue
        if 1 <= struct.unpack_from("<i", raw, i)[0] <= 300000:
            n += 1
    return n


def diff(a_label: str, b_label: str, limit: int) -> None:
    a, b = load(a_label), load(b_label)
    print(f"== {a_label} ({a['when']}) -> {b_label} ({b['when']})")
    for v in sorted(set(map(str, a["ints"])) | set(map(str, b["ints"]))):
        ha = {h["addr"]: h for h in a["hits"].get(v, [])}
        hb = {h["addr"]: h for h in b["hits"].get(v, [])}
        novos = [hb[x] for x in hb if x not in ha]
        sumiram = [ha[x] for x in ha if x not in hb]
        print(f"\n-- int {v}: A={len(ha)}  B={len(hb)}  novos={len(novos)}  sumiram={len(sumiram)}")
        # candidatos a lista: ocorrência nova com vizinhos que parecem ids
        novos.sort(key=lambda h: -plausible_ids(h["ctx"], h["ctx_off"]))
        for h in novos[:limit]:
            print(
                f"  NOVO  0x{h['addr']:X} (região 0x{h['region']:X}+0x{h['off']:X}, "
                f"vizinhos-id={plausible_ids(h['ctx'], h['ctx_off'])})"
            )
            print(f"        {slots(h['ctx'], h['ctx_off'])}")
        for h in sumiram[: max(3, limit // 4)]:
            print(f"  SUMIU 0x{h['addr']:X} (região 0x{h['region']:X}+0x{h['off']:X})")

    sa = {(s["text"], s["enc"]) for s in a["strings"]}
    sb = {(s["text"], s["enc"]) for s in b["strings"]}
    ca = {}
    cb = {}
    for s in a["strings"]:
        ca[s["text"]] = ca.get(s["text"], 0) + 1
    for s in b["strings"]:
        cb[s["text"]] = cb.get(s["text"], 0) + 1
    print("\n-- strings CM_* (contagem A -> B, só as que mudaram)")
    for t in sorted(set(ca) | set(cb)):
        if ca.get(t, 0) != cb.get(t, 0):
            print(f"  {t}: {ca.get(t, 0)} -> {cb.get(t, 0)}")
    addrs_a = {s["addr"] for s in a["strings"]}
    novas = [s for s in b["strings"] if s["addr"] not in addrs_a and "GTN" in s["text"]]
    for s in novas[:limit]:
        print(f"  NOVA 0x{s['addr']:X}  {s['text']}")
    _ = (sa, sb)


def list_runs() -> None:
    for p in sorted(RUNS_DIR.glob("*.json")):
        d = json.loads(p.read_text(encoding="utf-8"))
        print(f"{d['label']:>12}  {d['when']}  ints={d['ints']}  strings={len(d['strings'])}")


def selftest() -> None:
    """Planta um int32 raro e uma string CM_ no próprio processo e confere."""
    magic = 0x2B5E_C0DE & 0x7FFFFFFF
    arr = (ctypes.c_int32 * 8)(1, 2, magic, 4, 5, 6, 7, 8)
    text = ctypes.create_string_buffer(b"CM_Email_GTN_SelfTest\x00")
    pid = os.getpid()
    frase_ascii = ctypes.create_string_buffer(b"Negociar para assinar contrato (selftest)\x00")
    frase_utf16 = ctypes.create_unicode_buffer("Conversar sobre empréstimo (selftest)")
    path = capture(pid, "_selftest", [magic], max_mb=256, textos=["assinar contrato (selftest", "sobre empréstimo (selftest"])
    d = json.loads(path.read_text(encoding="utf-8"))
    want = ctypes.addressof(arr) + 8
    got = {h["addr"] for h in d["hits"][str(magic)]}
    ok_int = want in got
    ok_str = any(s["text"] == "CM_Email_GTN_SelfTest" for s in d["strings"])
    textos = {(x["enc"], x["text"]) for x in d["strings"] if x["text"].startswith("T:")}
    ok_ascii = any(e == "ascii" and "assinar contrato (selftest" in t for e, t in textos)
    ok_utf16 = any(e == "utf16" and "sobre empr" in t and "(selftest" in t for e, t in textos)
    print(
        "selftest int:", "OK" if ok_int else "FALHOU",
        "| string:", "OK" if ok_str else "FALHOU",
        "| texto ASCII:", "OK" if ok_ascii else "FALHOU",
        "| texto UTF-16:", "OK" if ok_utf16 else "FALHOU",
    )
    path.unlink()
    _ = (text, frase_ascii, frase_utf16)
    sys.exit(0 if ok_int and ok_str and ok_ascii and ok_utf16 else 1)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("capture")
    c.add_argument("label")
    c.add_argument("--int", dest="ints", type=int, action="append", default=[])
    c.add_argument("--seq", dest="seqs", action="append", default=[], help="int32 contíguos, separados por vírgula")
    c.add_argument("--str", dest="textos", action="append", default=[], help="texto da interface a procurar (ASCII e UTF-16)")
    c.add_argument("--pid", type=int)
    c.add_argument("--max-mb", type=int, default=256)
    d = sub.add_parser("diff")
    d.add_argument("a")
    d.add_argument("b")
    d.add_argument("--limit", type=int, default=25)
    sub.add_parser("list")
    sub.add_parser("selftest")
    args = ap.parse_args()

    if args.cmd == "capture":
        pid = args.pid or process.find_process_by_name(PROCESS_NAME)
        if pid is None:
            sys.exit("[ERRO] FIFA16.exe não está rodando.")
        capture(pid, args.label, args.ints, args.max_mb, [[int(x) for x in q.split(",")] for q in args.seqs], args.textos)
    elif args.cmd == "diff":
        diff(args.a, args.b, args.limit)
    elif args.cmd == "list":
        list_runs()
    else:
        selftest()


if __name__ == "__main__":
    main()
