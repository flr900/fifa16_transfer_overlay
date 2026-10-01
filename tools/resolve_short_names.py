"""Imprime os short names (4 chars) dos campos usados por `save_repo.rs`.

Uso (na máquina com o FIFA 16 instalado, onde existe o metadata XML):

    python tools/resolve_short_names.py [caminho/fifa_ng_db-meta.xml]

Cola a saída no bloco `mod fields` de `fifa_overlay/src/save_repo.rs`
(Story 1.1, Task 1.1). Imprime também TODOS os campos de `mPrV` para
ajudar a escolher um identificador numérico do manager caso
firstname/surname sejam strings (Task 1.4).
"""
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

DEFAULT_META = Path("D:/Program Files/FIFA 16/data/db/fifa_ng_db-meta.xml")

# (tabela_shortname, nome_do_campo_no_xml, constante_rust)
WANTED = [
    ("GJUr", "currdate", "GJUR_CURRDATE"),
    ("GJUr", "startdate", "GJUR_STARTDATE"),
    ("mPrV", "firstname", "MPRV_FIRSTNAME"),
    ("mPrV", "surname", "MPRV_SURNAME"),
    ("mPrV", "clubteamid", "MPRV_CLUBTEAMID"),
    ("dqXv", "transferbudget", "DQXV_TRANSFERBUDGET"),
]


def main() -> int:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_META
    if not path.exists():
        print(f"metadata XML não encontrado: {path}", file=sys.stderr)
        return 1

    root = ET.parse(path).getroot()
    tables = {t.get("shortname"): t for t in root.findall(".//table") if t.get("shortname")}

    print("// --- cole em `mod fields` (save_repo.rs) ---")
    for table_short, field_name, const_name in WANTED:
        table = tables.get(table_short)
        if table is None:
            print(f"// !! tabela {table_short} não existe no XML")
            continue
        match = None
        for f in table.findall("./fields/field"):
            if (f.get("name") or "").lower() == field_name:
                match = f
                break
        if match is None:
            print(f"// !! campo {table_short}.{field_name} não existe no XML")
            continue
        short = match.get("shortname")
        low = int(match.get("rangelow", "0"))
        print(
            f'pub const {const_name}: FieldRef = FieldRef {{ table: *b"{table_short}", '
            f'field: *b"{short}", range_low: {low} }}; // {match.get("type")}'
        )

    print("\n// --- todos os campos de mPrV (para Task 1.4) ---")
    mprv = tables.get("mPrV")
    if mprv is not None:
        for f in mprv.findall("./fields/field"):
            print(f'// {f.get("shortname")}  {f.get("name")}  type={f.get("type")}  rangelow={f.get("rangelow", "0")}')
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
