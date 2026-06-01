#!/usr/bin/env python3
"""Bootstrap validator for mdatron's bootstrap period.

Throwaway script per BOOTSTRAP-MITIGATION.md § Mitigation 1.
Covers 5 cascade-prone structural-drift patterns from the 2026-06-01 review:

  Class 1 — Cross-document count drift          (BOOT-W0001)
  Class 2 — Orphaned rules / table rows         (BOOT-W0002)
  Class 3 — Broken intra-doc hyperlinks         (BOOT-W0003)
  Class 4 — Header-vs-table count inconsistency (BOOT-W0004)
  Class 6 — Frontmatter schema_class mistyped   (BOOT-W0006)

Deleted partway through Step 3 when `mdatron-core verify` operates against
the project. Not a substitute for mdatron; bounded coverage of the most
cascade-prone patterns until the real engine is online.

Usage:
  python bootstrap-validator.py <path> [<path>...]
  python bootstrap-validator.py .

Exit 0 if no findings; 1 if any error-severity findings.
"""

import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

try:
    import yaml
except ImportError:
    print("bootstrap-validator: missing pyyaml; install via `pip install pyyaml`", file=sys.stderr)
    sys.exit(2)


@dataclass
class Finding:
    code: str
    severity: str  # "error" | "warning" | "lint"
    file: Path
    line: int
    message: str

    def format_rustc(self) -> str:
        return f"{self.severity}[{self.code}]: {self.message}\n  --> {self.file}:{self.line}"


FRONTMATTER_RE = re.compile(r"\A---\n(.*?)\n---", re.DOTALL)


def parse_frontmatter(content: str):
    m = FRONTMATTER_RE.match(content)
    if not m:
        return None
    try:
        return yaml.safe_load(m.group(1))
    except yaml.YAMLError:
        return None


def slugify(text: str) -> str:
    s = text.lower().strip()
    s = re.sub(r"\s+", "-", s)
    s = re.sub(r"[^a-z0-9-]", "", s)
    return s


def line_of(content: str, offset: int) -> int:
    return content[:offset].count("\n") + 1


def collect_files(roots: list[str]) -> list[Path]:
    files: list[Path] = []
    skip_dirs = {".crosslink", ".git", ".claude", "node_modules", "target"}
    for root in roots:
        p = Path(root)
        if p.is_file() and p.suffix == ".md":
            files.append(p)
        elif p.is_dir():
            for f in p.rglob("*.md"):
                if any(part in skip_dirs for part in f.parts):
                    continue
                files.append(f)
    return sorted(set(files))


# ── Class 1: cross-document count drift ────────────────────────────────────────

def check_count_drift(files: list[Path], manifest: dict) -> list[Finding]:
    findings: list[Finding] = []
    for spec in manifest.get("counts", []):
        canonical = spec["canonical"]
        pattern = re.compile(spec["pattern"], re.IGNORECASE)
        for f in files:
            content = f.read_text(encoding="utf-8")
            for m in pattern.finditer(content):
                found = int(m.group(1))
                if found != canonical:
                    findings.append(Finding(
                        code="BOOT-W0001",
                        severity="warning",
                        file=f,
                        line=line_of(content, m.start()),
                        message=f"count drift: '{m.group(0)}' diverges from canonical {canonical} ({spec['name']})",
                    ))
    return findings


# ── Class 2: orphan reference-table entries ────────────────────────────────────

def check_orphan_rules(files: list[Path], manifest: dict) -> list[Finding]:
    findings: list[Finding] = []
    next_h_re = re.compile(r"^#+\s+", re.MULTILINE)
    sep_re = re.compile(r"^\s*\|[-:|\s]+\|\s*$")
    first_cell_re = re.compile(r"^\s*\|\s*\*?\*?`?([^|`*]+?)`?\*?\*?\s*\|")
    for table in manifest.get("reference_tables", []):
        target_suffix = table["file_suffix"]
        heading_pattern = re.compile(rf"^#+\s+{re.escape(table['section_heading'])}.*$", re.MULTILINE)
        live = {entry.lower() for entry in table["live_entries"]}
        for f in files:
            if not str(f).endswith(target_suffix):
                continue
            content = f.read_text(encoding="utf-8")
            heading_match = heading_pattern.search(content)
            if not heading_match:
                continue
            section_start = heading_match.end()
            next_match = next_h_re.search(content, section_start)
            section_end = next_match.start() if next_match else len(content)
            section_text = content[section_start:section_end]
            # Walk lines; track table state. Header row precedes separator; data rows follow.
            in_data = False
            for i, ln in enumerate(section_text.split("\n")):
                if sep_re.match(ln):
                    in_data = True
                    continue
                if not ln.lstrip().startswith("|"):
                    in_data = False
                    continue
                if not in_data:
                    continue  # header row; skip
                m = first_cell_re.match(ln)
                if not m:
                    continue
                cell = m.group(1).strip()
                if not cell or cell.lower() in live:
                    continue
                line_num = line_of(content, section_start) + i
                findings.append(Finding(
                    code="BOOT-W0002",
                    severity="warning",
                    file=f,
                    line=line_num,
                    message=f"orphan reference: '{cell}' in section '{table['section_heading']}' not in live entries",
                ))
    return findings


# ── Class 3: broken intra-doc hyperlinks ───────────────────────────────────────

def build_anchor_index(files: list[Path]) -> dict[str, list[Path]]:
    index: dict[str, list[Path]] = {}
    heading_re = re.compile(r"^#{1,6}\s+(.+?)\s*$", re.MULTILINE)
    for f in files:
        content = f.read_text(encoding="utf-8")
        for m in heading_re.finditer(content):
            slug = slugify(m.group(1))
            index.setdefault(slug, []).append(f)
    return index


def check_broken_anchors(files: list[Path], anchor_index: dict) -> list[Finding]:
    findings: list[Finding] = []
    link_re = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
    for f in files:
        content = f.read_text(encoding="utf-8")
        for m in link_re.finditer(content):
            target = m.group(2).strip()
            if not target.startswith("#"):
                continue
            anchor = target.lstrip("#").split("?")[0].split(" ")[0]
            if not anchor:
                continue
            if anchor not in anchor_index:
                findings.append(Finding(
                    code="BOOT-W0003",
                    severity="warning",
                    file=f,
                    line=line_of(content, m.start()),
                    message=f"broken anchor: '#{anchor}' does not resolve to any heading slug in the project",
                ))
    return findings


# ── Class 4: header-vs-table count ─────────────────────────────────────────────

def check_header_table_count(files: list[Path]) -> list[Finding]:
    findings: list[Finding] = []
    header_re = re.compile(r"^(#{2,6})\s+(.+?)\s+\((\d+)\)\s*$", re.MULTILINE)
    in_code = re.compile(r"```")
    for f in files:
        content = f.read_text(encoding="utf-8")
        # Build a set of line numbers inside code fences to skip
        code_lines: set[int] = set()
        in_block = False
        for i, ln in enumerate(content.split("\n"), start=1):
            if in_code.search(ln):
                in_block = not in_block
                code_lines.add(i)
            elif in_block:
                code_lines.add(i)
        for m in header_re.finditer(content):
            line = line_of(content, m.start())
            if line in code_lines:
                continue
            declared = int(m.group(3))
            after = content[m.end():]
            next_h = re.search(r"^#+\s+", after, re.MULTILINE)
            section = after[: next_h.start()] if next_h else after
            # Try a table first
            table_lines = [ln for ln in section.split("\n") if ln.lstrip().startswith("|")]
            if len(table_lines) >= 3:  # header + separator + ≥1 data
                data_rows = [ln for ln in table_lines[2:] if not re.match(r"^\s*\|\s*[-:]+", ln)]
                actual = len(data_rows)
                if actual != declared:
                    findings.append(Finding(
                        code="BOOT-W0004",
                        severity="warning",
                        file=f,
                        line=line,
                        message=f"header declares ({declared}) but following table has {actual} data row(s)",
                    ))
                continue
            # Fall back to bullet list
            items = re.findall(r"^[-*]\s+\S", section, re.MULTILINE)
            if items:
                if len(items) != declared:
                    findings.append(Finding(
                        code="BOOT-W0004",
                        severity="warning",
                        file=f,
                        line=line,
                        message=f"header declares ({declared}) but following list has {len(items)} item(s)",
                    ))
    return findings


# ── Class 6: frontmatter schema_class mistyped ────────────────────────────────

def check_schema_class(files: list[Path], manifest: dict) -> list[Finding]:
    findings: list[Finding] = []
    valid = set(manifest.get("schema_classes", []))
    for f in files:
        content = f.read_text(encoding="utf-8")
        fm = parse_frontmatter(content)
        if not fm or not isinstance(fm, dict):
            continue
        sc = fm.get("schema_class")
        if sc is None:
            continue
        if sc not in valid:
            findings.append(Finding(
                code="BOOT-W0006",
                severity="warning",
                file=f,
                line=2,
                message=f"frontmatter schema_class '{sc}' not in registered set",
            ))
    return findings


# ── Main ───────────────────────────────────────────────────────────────────────

def load_yaml(path: Path) -> dict:
    if not path.exists():
        return {}
    with open(path, encoding="utf-8") as fh:
        return yaml.safe_load(fh) or {}


def main() -> int:
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <path> [<path>...]", file=sys.stderr)
        return 2

    script_dir = Path(__file__).parent
    counts_manifest = load_yaml(script_dir / "canonical-counts.yaml")
    entities_manifest = load_yaml(script_dir / "live-entities.yaml")

    files = collect_files(sys.argv[1:])
    if not files:
        print("bootstrap-validator: no markdown files found", file=sys.stderr)
        return 0

    anchor_index = build_anchor_index(files)

    findings: list[Finding] = []
    findings.extend(check_count_drift(files, counts_manifest))
    findings.extend(check_orphan_rules(files, entities_manifest))
    findings.extend(check_broken_anchors(files, anchor_index))
    findings.extend(check_header_table_count(files))
    findings.extend(check_schema_class(files, entities_manifest))

    # Sort for deterministic output
    findings.sort(key=lambda f: (str(f.file), f.line, f.code))

    if not findings:
        print(f"bootstrap-validator: {len(files)} file(s) checked; no findings")
        return 0

    for finding in findings:
        print(finding.format_rustc())
        print()

    errors = sum(1 for f in findings if f.severity == "error")
    warnings = sum(1 for f in findings if f.severity == "warning")
    print(f"bootstrap-validator: {errors} error(s), {warnings} warning(s) across {len(files)} file(s)")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
