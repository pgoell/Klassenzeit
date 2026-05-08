"""Render docs/OPEN_THINGS.md to a single self-contained, readable HTML page.

Usage:
    uv run python scripts/render_open_things_html.py            # write
    uv run python scripts/render_open_things_html.py --check    # diff-only, exit 1 on drift

The HTML is committed alongside the markdown so the user can read it in
a browser without running a build. The pre-commit hook regenerates it
on every commit that touches OPEN_THINGS.md; CI's `check:openthings-html`
catches drift if the hook is bypassed.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

import markdown

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC = REPO_ROOT / "docs" / "OPEN_THINGS.md"
DST = REPO_ROOT / "docs" / "OPEN_THINGS.html"


def preprocess(md_text: str) -> str:
    """Pin numbered-item numbers as explicit text, attach anchors.

    Python markdown renders ordered lists with browser auto-numbering
    starting at 1, so item 61 would render as "1." Preprocess each
    numbered item line so the canonical number stays visible and
    the item gets an `item-N` anchor for cross-linking.
    """
    pattern = re.compile(r"^(\d+)\. \*\*([^*]+?)\*\*", re.MULTILINE)

    def repl(m: re.Match[str]) -> str:
        num, title = m.groups()
        return f'<a class="item-anchor" id="item-{num}"></a>**[{num}] {title}**'

    return pattern.sub(repl, md_text)


def render() -> str:
    """Read OPEN_THINGS.md, render to HTML, return the full HTML document text."""
    md_text = SRC.read_text()
    md_text = preprocess(md_text)

    body = markdown.markdown(
        md_text,
        extensions=["extra", "toc", "sane_lists"],
        extension_configs={"toc": {"toc_depth": "2-3", "permalink": False}},
    )

    body = re.sub(
        r"<code>\[(P[012])\]</code>",
        r'<span class="prio prio-\1">\1</span>',
        body,
    )

    return TEMPLATE.format(body=body)


TEMPLATE = r"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Klassenzeit: Open Things</title>
<style>
  :root {{
    --bg: #fafaf7;
    --bg-soft: #f3f3ee;
    --fg: #1c1d1a;
    --fg-soft: #5b5d57;
    --accent: #5e7a4a;
    --border: #d8d6cf;
    --code-bg: #ecebe4;
    --p0: #c0392b;
    --p1: #b78316;
    --p2: #7c7e76;
    --max-w: 760px;
  }}
  @media (prefers-color-scheme: dark) {{
    :root {{
      --bg: #181815;
      --bg-soft: #21211d;
      --fg: #e7e6df;
      --fg-soft: #9a998f;
      --accent: #93b378;
      --border: #34342e;
      --code-bg: #2a2a25;
    }}
  }}
  * {{ box-sizing: border-box; }}
  html {{ scroll-behavior: smooth; scroll-padding-top: 1rem; }}
  body {{
    margin: 0;
    background: var(--bg);
    color: var(--fg);
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", "Helvetica Neue", system-ui, sans-serif;
    font-size: 16px;
    line-height: 1.65;
    display: grid;
    grid-template-columns: 280px 1fr;
    min-height: 100vh;
  }}
  aside {{
    position: sticky;
    top: 0;
    height: 100vh;
    overflow-y: auto;
    padding: 1.5rem 1rem 1rem 1.5rem;
    background: var(--bg-soft);
    border-right: 1px solid var(--border);
    font-size: 0.92rem;
  }}
  aside h2 {{
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--fg-soft);
    margin: 0 0 0.5rem 0;
  }}
  aside .toc ul {{ list-style: none; padding-left: 0; margin: 0; }}
  aside .toc > .toc > ul > li {{ margin-top: 0.6rem; }}
  aside .toc ul ul {{ padding-left: 1rem; font-size: 0.88em; }}
  aside .toc a {{
    color: var(--fg);
    text-decoration: none;
    display: block;
    padding: 0.15rem 0.4rem;
    border-radius: 3px;
  }}
  aside .toc a:hover {{ background: var(--code-bg); color: var(--accent); }}
  aside #jump {{
    width: 100%;
    margin-bottom: 1rem;
    padding: 0.4rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    font-size: 0.9rem;
  }}
  main {{
    padding: 2rem 3rem 6rem 3rem;
    max-width: calc(var(--max-w) + 6rem);
  }}
  h1 {{
    margin-top: 0;
    font-size: 2rem;
    border-bottom: 2px solid var(--border);
    padding-bottom: 0.5rem;
  }}
  h2 {{
    margin-top: 2.5rem;
    font-size: 1.45rem;
    color: var(--accent);
    border-bottom: 1px solid var(--border);
    padding-bottom: 0.3rem;
  }}
  h3 {{ margin-top: 1.8rem; font-size: 1.15rem; color: var(--fg-soft); }}
  h4 {{ margin-top: 1.5rem; font-size: 1.02rem; }}
  p {{ margin: 0.7rem 0; }}
  a {{ color: var(--accent); }}
  code {{
    background: var(--code-bg);
    padding: 0.1em 0.35em;
    border-radius: 3px;
    font-size: 0.92em;
    font-family: "JetBrains Mono", "Fira Code", "SF Mono", Menlo, monospace;
  }}
  pre {{
    background: var(--code-bg);
    padding: 0.8rem 1rem;
    border-radius: 5px;
    overflow-x: auto;
    font-size: 0.88em;
  }}
  pre code {{ padding: 0; background: none; }}
  ul, ol {{ padding-left: 1.6rem; }}
  ul li, ol li {{ margin: 0.45rem 0; }}
  hr {{ border: none; border-top: 1px solid var(--border); margin: 2rem 0; }}
  blockquote {{
    border-left: 3px solid var(--accent);
    margin: 1rem 0;
    padding: 0.3rem 0 0.3rem 1rem;
    color: var(--fg-soft);
  }}
  .prio {{
    display: inline-block;
    padding: 0.05em 0.45em;
    border-radius: 3px;
    font-family: "JetBrains Mono", "Fira Code", "SF Mono", Menlo, monospace;
    font-size: 0.78em;
    font-weight: 600;
    color: white;
    vertical-align: 1px;
  }}
  .prio-P0 {{ background: var(--p0); }}
  .prio-P1 {{ background: var(--p1); }}
  .prio-P2 {{ background: var(--p2); }}
  .item-anchor {{ display: block; height: 0; visibility: hidden; }}
  .hidden {{ display: none !important; }}
  mark {{
    background: rgba(255, 217, 64, 0.45);
    color: inherit;
    padding: 0 2px;
    border-radius: 2px;
  }}
  @media (max-width: 900px) {{
    body {{ grid-template-columns: 1fr; }}
    aside {{
      position: static;
      height: auto;
      border-right: none;
      border-bottom: 1px solid var(--border);
    }}
    main {{ padding: 1.5rem 1.2rem 4rem 1.2rem; }}
  }}
</style>
</head>
<body>
<aside>
  <input id="jump" type="search" placeholder="Jump to item N or filter text…" autocomplete="off">
  <h2>Contents</h2>
  <nav class="toc" id="toc-nav"></nav>
</aside>
<main>
{body}
</main>
<script>
  (function buildToc() {{
    var nav = document.getElementById('toc-nav');
    var headings = document.querySelectorAll('main h2, main h3');
    var ul = document.createElement('ul');
    var currentSubUl = null;
    var slug = function (text) {{
      return text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
    }};
    headings.forEach(function (h) {{
      if (!h.id) h.id = slug(h.textContent);
      var li = document.createElement('li');
      var a = document.createElement('a');
      a.href = '#' + h.id;
      a.textContent = h.textContent;
      li.appendChild(a);
      if (h.tagName === 'H2') {{
        ul.appendChild(li);
        currentSubUl = document.createElement('ul');
        li.appendChild(currentSubUl);
      }} else {{
        if (currentSubUl) currentSubUl.appendChild(li);
        else ul.appendChild(li);
      }}
    }});
    nav.appendChild(ul);
  }})();

  (function filter() {{
    var input = document.getElementById('jump');
    var paragraphs = Array.prototype.slice.call(document.querySelectorAll('main p, main li'));
    input.addEventListener('keydown', function (e) {{
      if (e.key === 'Enter') {{
        var v = input.value.trim();
        if (/^\d+$/.test(v)) {{
          var el = document.getElementById('item-' + v);
          if (el) {{
            el.scrollIntoView({{block: 'start'}});
            el.parentElement.style.background = 'rgba(94,122,74,0.12)';
            setTimeout(function () {{ el.parentElement.style.background = ''; }}, 1500);
          }}
        }}
        e.preventDefault();
      }}
    }});
    input.addEventListener('input', function () {{
      var v = input.value.trim().toLowerCase();
      if (!v) {{
        paragraphs.forEach(function (p) {{ p.classList.remove('hidden'); }});
        return;
      }}
      if (/^\d+$/.test(v)) return;
      paragraphs.forEach(function (p) {{
        if (p.textContent.toLowerCase().indexOf(v) === -1) p.classList.add('hidden');
        else p.classList.remove('hidden');
      }});
    }});
  }})();
</script>
</body>
</html>
"""


def main() -> int:
    """CLI entry point. Returns 0 on success, 1 on --check drift or missing file."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Exit 1 if docs/OPEN_THINGS.html does not match the regenerated content.",
    )
    args = parser.parse_args()

    fresh = render()

    if args.check:
        if not DST.exists():
            print(
                f"{DST.relative_to(REPO_ROOT)} is missing; run `mise run gen:openthings-html`.",
                file=sys.stderr,
            )
            return 1
        current = DST.read_text()
        if current != fresh:
            print(
                f"{DST.relative_to(REPO_ROOT)} is out of sync with {SRC.relative_to(REPO_ROOT)}; "
                f"run `mise run gen:openthings-html`.",
                file=sys.stderr,
            )
            return 1
        return 0

    DST.write_text(fresh)
    print(f"wrote {DST.relative_to(REPO_ROOT)} ({len(fresh):,} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
