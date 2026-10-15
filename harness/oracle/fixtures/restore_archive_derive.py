"""P4.D293: the shared rewrite step of the `derive-restore-archive-*.py`
scripts this lane adds — copy a committed source archive entry by entry,
replacing the `data/<stem>.json` files a plan rewrites (and adding any the
source lacks), the source md5-checked unchanged before and after. The
P4.D273 `derive-restore-archive-memory-tiers.py` shape, factored once.
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
BASE = ARCHIVES / "restore-archive.zip"
BASE_MD5 = "6b076003bacf8db22c1bf8440fac09d6"


def md5(path):
    return hashlib.md5(path.read_bytes()).hexdigest()


def load(zin, root, name):
    return json.loads(zin.read(f"{root}/data/{name}.json"))


def derive(dst_name, plan, src=BASE, src_md5=BASE_MD5):
    """Write `dst_name` from `src`; `plan(zin, root)` returns `{stem: rows}`."""
    dst = ARCHIVES / dst_name
    before = md5(src)
    assert before == src_md5, f"{src.name} moved: {before}"
    with zipfile.ZipFile(src) as zin:
        root = zin.infolist()[0].filename.rstrip("/")
        rewritten = plan(zin, root)
        written = set()
        with zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                rel = info.filename[len(root) + 1:]
                data = zin.read(info.filename)
                stem = rel[len("data/"):-len(".json")] if rel.startswith("data/") else None
                if stem in rewritten:
                    data = (json.dumps(rewritten[stem], indent=2, ensure_ascii=False) + "\n").encode()
                    written.add(stem)
                out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                out.external_attr = info.external_attr
                out.compress_type = zipfile.ZIP_DEFLATED
                zout.writestr(out, data)
            for stem in sorted(set(rewritten) - written):
                out = zipfile.ZipInfo(f"{root}/data/{stem}.json", date_time=(2026, 7, 25, 6, 31, 24))
                out.external_attr = 0o644 << 16
                out.compress_type = zipfile.ZIP_DEFLATED
                zout.writestr(out, (json.dumps(rewritten[stem], indent=2, ensure_ascii=False) + "\n").encode())
    after = md5(src)
    assert after == src_md5, f"{src.name} moved during the derive: {after}"
    print(f"wrote {dst.relative_to(ROOT)} ({src.name} md5 {after} unchanged)")
