"""SMCL (Stata Markup and Control Language) to HTML converter.

Converts Stata help files (.sthlp) written in SMCL markup to HTML
for display in VS Code webview panels with clickable links and formatting.
"""

import re
import os
import logging


def _html_esc(text):
    """Escape HTML special characters."""
    return (text.replace('&', '&amp;').replace('<', '&lt;')
            .replace('>', '&gt;').replace('"', '&quot;'))


# ── Character code mappings for {c code} ────────────────────────────────
_CHAR_CODES = {
    'S|': '$', "'g": '`', '-(': '{', ')-': '}',
    '-': '\u2500', '|': '\u2502', '+': '\u253c',
    'TT': '\u252c', 'BT': '\u2534', 'LT': '\u251c', 'RT': '\u2524',
    'TLC': '\u250c', 'TRC': '\u2510', 'BRC': '\u2518', 'BLC': '\u2514',
    # Accented characters
    "a'": '\u00e1', "A'": '\u00c1', "e'": '\u00e9', "E'": '\u00c9',
    "i'": '\u00ed', "I'": '\u00cd', "o'": '\u00f3', "O'": '\u00d3',
    "u'": '\u00fa', "U'": '\u00da',
    "a'g": '\u00e0', "A'g": '\u00c0', "e'g": '\u00e8', "E'g": '\u00c8',
    "i'g": '\u00ec', "I'g": '\u00cc', "o'g": '\u00f2', "O'g": '\u00d2',
    "u'g": '\u00f9', "U'g": '\u00d9',
    "a^": '\u00e2', "A^": '\u00c2', "e^": '\u00ea', "E^": '\u00ca',
    "i^": '\u00ee', "I^": '\u00ce', "o^": '\u00f4', "O^": '\u00d4',
    "u^": '\u00fb', "U^": '\u00db',
    "a~": '\u00e3', "A~": '\u00c3', "n~": '\u00f1', "N~": '\u00d1',
    "o~": '\u00f5', "O~": '\u00d5',
    "a..": '\u00e4', "A..": '\u00c4', "e..": '\u00eb', "E..": '\u00cb',
    "o..": '\u00f6', "O..": '\u00d6', "u..": '\u00fc', "U..": '\u00dc',
    "ss": '\u00df', "c,": '\u00e7', "C,": '\u00c7',
}


def _resolve_char(code):
    """Resolve a {c code} directive to its character."""
    code = code.strip()
    if code in _CHAR_CODES:
        return _CHAR_CODES[code]
    if code.startswith('0x') or code.startswith('0X'):
        try:
            return chr(int(code[2:], 16))
        except (ValueError, OverflowError):
            return code
    try:
        n = int(code)
        if 1 <= n <= 0x10FFFF:
            return chr(n)
    except (ValueError, OverflowError):
        pass
    return code


def _find_brace(text, start):
    """Find index of } matching the { at position start. Returns -1 if unmatched."""
    depth = 1
    i = start + 1
    while i < len(text):
        if text[i] == '{':
            depth += 1
        elif text[i] == '}':
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1


def _parse_tag(content):
    """Parse content inside {...} into (name, args, inner_text).

    Returns (name, args_str, inner_text_or_None).
    - {name}            -> (name, '', None)
    - {name:text}       -> (name, '', text)
    - {name args}       -> (name, args, None)
    - {name args:text}  -> (name, args, text)
    """
    content = content.strip()
    if not content:
        return ('', '', None)
    if content.startswith('*'):
        return ('*', content[1:], None)
    if content == '...':
        return ('...', '', None)
    if content == '.-':
        return ('.-', '', None)

    m = re.match(r'^([a-zA-Z_][a-zA-Z0-9_]*)', content)
    if not m:
        return ('', content, None)

    name = m.group(1)
    rest = content[m.end():]
    if not rest:
        return (name, '', None)

    # Syntax 2: {name:text}
    if rest[0] == ':':
        return (name, '', rest[1:])

    # Syntax 3/4: {name args} or {name args:text}
    if rest[0] == ' ':
        args_part = rest[1:]
        # Find first ':' not inside nested braces or quoted strings
        depth = 0
        in_quote = False
        for idx, ch in enumerate(args_part):
            if ch == '"' and depth == 0:
                in_quote = not in_quote
            elif not in_quote:
                if ch == '{':
                    depth += 1
                elif ch == '}':
                    depth -= 1
                elif ch == ':' and depth == 0:
                    return (name, args_part[:idx].strip(), args_part[idx + 1:])
        return (name, args_part.strip(), None)

    # Fallback
    return (name, rest, None)


# ── CSS for the help page ────────────────────────────────────────────────
_CSS = r"""
* { box-sizing: border-box; margin: 0; padding: 0; }
body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
    font-size: 14px; line-height: 1.55;
    color: var(--vscode-editor-foreground);
    background: var(--vscode-editor-background);
    padding: 16px 24px 40px;
}
/* ── Navigation ── */
.smcl-toc {
    background: var(--vscode-editorWidget-background, var(--vscode-sideBar-background));
    border: 1px solid var(--vscode-editorWidget-border, var(--vscode-panel-border, transparent));
    border-radius: 4px; padding: 8px 14px; margin-bottom: 16px;
    font-size: 13px; line-height: 1.8;
}
.smcl-toc a { margin: 0 2px; }
.smcl-alsosee {
    margin-top: 24px; padding-top: 12px;
    border-top: 1px solid var(--vscode-editorGroup-border, #444);
    font-size: 13px;
}
.smcl-alsosee a { margin: 0 3px; }
.smcl-alsosee-sep { margin: 0 4px; opacity: 0.4; }
/* ── Headings ── */
h2.smcl-title {
    font-size: 15px; font-weight: 600; margin: 22px 0 6px;
    padding-bottom: 3px;
    border-bottom: 1px solid var(--vscode-editorGroup-border, #444);
}
h3.smcl-dlgtab {
    font-size: 14px; font-weight: 600; margin: 14px 0 4px;
    padding: 3px 8px;
    background: var(--vscode-editorWidget-background, var(--vscode-sideBar-background));
    border-radius: 3px;
}
/* ── Header (title line like [R] regress) ── */
.smcl-header {
    font-size: 15px; margin-bottom: 4px;
}
/* ── Inline styles ── */
.smcl-cmd {
    font-family: 'SF Mono', 'Fira Code', Menlo, Consolas, 'Courier New', monospace;
    font-weight: 600;
    color: var(--vscode-textLink-foreground);
}
.smcl-err { color: var(--vscode-errorForeground, #f44); }
.smcl-res { font-weight: 600; }
.smcl-com { color: var(--vscode-descriptionForeground, #6a9955); font-family: 'SF Mono', Menlo, Consolas, monospace; }
.smcl-hilite { background: var(--vscode-editor-findMatchHighlightBackground, rgba(255,200,0,.25)); padding: 0 2px; border-radius: 2px; }
.smcl-mansection { font-style: italic; }
.smcl-stata-cmd { font-family: 'SF Mono', Menlo, Consolas, monospace; }
/* ── Links ── */
a.smcl-help-link, a.smcl-browse-link {
    color: var(--vscode-textLink-foreground);
    text-decoration: none; cursor: pointer;
}
a.smcl-help-link:hover, a.smcl-browse-link:hover { text-decoration: underline; }
/* ── Paragraphs ── */
.smcl-pstd  { margin: 6px 0 6px 2em; }
.smcl-phang { margin: 4px 0 4px 2em; padding-left: 2em; text-indent: -2em; }
.smcl-phang2 { margin: 4px 0 4px 4em; padding-left: 2em; text-indent: -2em; }
.smcl-phang3 { margin: 4px 0 4px 6em; padding-left: 2em; text-indent: -2em; }
.smcl-pmore, .smcl-pin  { margin: 4px 0 4px 4em; }
.smcl-pmore2, .smcl-pin2 { margin: 4px 0 4px 6em; }
.smcl-pmore3, .smcl-pin3 { margin: 4px 0 4px 8em; }
.smcl-psee  { margin: 4px 0 4px 2em; padding-left: 5em; text-indent: -5em; }
.smcl-p     { margin: 4px 0; }
/* ── Lines ── */
.smcl-line { white-space: pre-wrap; font-family: 'SF Mono', Menlo, Consolas, monospace; font-size: 13px; min-height: 1em; }
hr.smcl-hline, hr.smcl-p2line {
    border: none; border-top: 1px solid var(--vscode-editorGroup-border, #444);
    margin: 4px 0;
}
.smcl-center { text-align: center; }
.smcl-right  { text-align: right; }
/* ── Two-column / synopt tables ── */
table.smcl-synopt-table {
    width: 100%; border-collapse: collapse; margin: 2px 0; font-size: 14px;
}
table.smcl-synopt-table th {
    text-align: left; padding: 2px 10px; font-weight: normal; font-style: italic;
    border-bottom: 1px solid var(--vscode-editorGroup-border, #444);
}
table.smcl-synopt-table td { padding: 2px 10px; vertical-align: top; }
td.smcl-synopt-col1 { white-space: nowrap; padding-right: 16px; min-width: 160px; }
tr.smcl-synopt-line td hr { border: none; border-top: 1px solid var(--vscode-editorGroup-border, #444); }
tr.smcl-syntab td { padding-top: 10px; font-weight: 600; }
/* ── p2col header rows ── */
.smcl-p2col { display: flex; gap: 8px; margin: 2px 0; }
.smcl-p2col-1 { flex-shrink: 0; }
/* ── Back / Forward buttons ── */
.smcl-nav-bar {
    display: flex; gap: 8px; margin-bottom: 10px; font-size: 13px;
}
.smcl-nav-bar button {
    background: var(--vscode-button-secondaryBackground, #333);
    color: var(--vscode-button-secondaryForeground, #ccc);
    border: none; border-radius: 3px; padding: 3px 10px; cursor: pointer; font-size: 12px;
}
.smcl-nav-bar button:hover { background: var(--vscode-button-secondaryHoverBackground, #444); }
.smcl-nav-bar button:disabled { opacity: 0.4; cursor: default; }
.smcl-nav-bar .smcl-nav-topic { margin-left: auto; opacity: 0.6; font-style: italic; }
"""

# ── JavaScript for link handling and navigation ──────────────────────────
_JS = r"""
(function() {
    const vscode = acquireVsCodeApi();

    document.addEventListener('click', function(e) {
        // Help links
        var link = e.target.closest('a.smcl-help-link');
        if (link) {
            e.preventDefault();
            var topic = link.dataset.topic;
            var marker = link.dataset.marker || '';
            if (topic) {
                vscode.postMessage({ command: 'helpNavigate', topic: topic, marker: marker });
            } else if (link.hash) {
                var el = document.getElementById(link.hash.substring(1));
                if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
            }
            return;
        }
        // Browse links (external URLs)
        link = e.target.closest('a.smcl-browse-link');
        if (link) {
            e.preventDefault();
            vscode.postMessage({ command: 'openExternal', url: link.href });
            return;
        }
        // Navigation buttons
        var btn = e.target.closest('button[data-action]');
        if (btn) {
            vscode.postMessage({ command: btn.dataset.action });
        }
    });

    // Messages from extension
    window.addEventListener('message', function(e) {
        var msg = e.data;
        if (msg.command === 'scrollToMarker' && msg.marker) {
            var el = document.getElementById(msg.marker);
            if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
        }
    });
})();
"""


class SmclParser:
    """Converts SMCL markup to HTML."""

    def __init__(self):
        self.toc = []           # [(display_text, target)]  from {viewerjumpto}
        self.also_see = []      # [(display_text, help_topic_or_empty)]
        self.markers = set()

    # ── Public API ───────────────────────────────────────────────────────

    def convert(self, raw_smcl, include_resolver=None, topic=''):
        """Convert raw SMCL text to a complete HTML page string."""
        text = self._preprocess(raw_smcl)
        if include_resolver:
            text = self._resolve_includes(text, include_resolver)
        text = self._extract_metadata(text)
        body = self._render_body(text)
        return self._wrap_html(body, topic)

    # ── Preprocessing ────────────────────────────────────────────────────

    def _preprocess(self, text):
        lines = text.replace('\r\n', '\n').replace('\r', '\n').split('\n')
        result = []
        for line in lines:
            s = line.strip()
            # Skip star-bang version comments
            if s.startswith('*!') or re.match(r'^\{\*\s*\*!', s):
                continue
            # Skip {smcl} mode line
            if s == '{smcl}':
                continue
            # Strip {...} continuation markers (display hint for Stata Viewer, not needed for parsing)
            if line.rstrip().endswith('{...}'):
                line = line.rstrip()[:-5]
            result.append(line)
        return '\n'.join(result)

    def _resolve_includes(self, text, resolver):
        lines = text.split('\n')
        result = []
        for line in lines:
            m = re.match(r'^(\s*)INCLUDE\s+help\s+(\S+)\s*$', line)
            if m:
                indent = m.group(1)
                name = m.group(2)
                content = resolver(name)
                if content:
                    # Recursively resolve nested includes
                    content = self._resolve_includes(content, resolver)
                    result.append(content)
                else:
                    result.append(f'{indent}<!-- INCLUDE help {name}: not found -->')
            else:
                result.append(line)
        return '\n'.join(result)

    def _extract_metadata(self, text):
        # Extract {viewerjumpto} entries from anywhere in the text
        for m in re.finditer(r'\{viewerjumpto\s+"([^"]+)"\s+"([^"]+)"\}', text):
            self.toc.append((m.group(1), m.group(2)))
        # Extract {vieweralsosee} entries
        for m in re.finditer(r'\{vieweralsosee\s+"([^"]*)"\s+"([^"]*)"\}', text):
            disp, target = m.group(1), m.group(2)
            if disp == '' and target == '--':
                self.also_see.append(('---', ''))
            elif target.startswith('help '):
                self.also_see.append((disp, target[5:].strip()))
            elif target.startswith('mansection '):
                self.also_see.append((disp, ''))
            elif target:
                self.also_see.append((disp, target))

        # Remove metadata lines from the text
        lines = text.split('\n')
        remaining = []
        for line in lines:
            s = line.strip()
            if re.match(r'^\{viewerjumpto\s', s):
                continue
            if re.match(r'^\{vieweralsosee\s', s):
                continue
            if s.startswith('{viewerdialog') or s.startswith('{findalias'):
                continue
            remaining.append(line)
        return '\n'.join(remaining)

    # ── Brace-aware helpers ────────────────────────────────────────────────

    @staticmethod
    def _parse_two_col_line(line, tag):
        """Parse a two-column directive like {synopt :col1}col2 with brace-aware matching.

        Returns (col1_raw, col2_raw) or None if not a match.
        tag should be 'synopt', 'p2col', or 'p2coldent'.
        """
        s = line.strip()
        # Match the opening tag
        m = re.match(r'^\{' + tag + r'(?:\s[\d\s]*)?\s*:', s)
        if not m:
            return None
        # Find the matching } for the opening { using brace counting
        start = 0  # position of the opening {
        end = _find_brace(s, start)
        if end == -1:
            return None
        # Everything between tag: and closing } is col1 content
        col1_start = m.end()
        col1 = s[col1_start:end]
        # Everything after closing } is col2
        col2 = s[end + 1:]
        return (col1, col2)

    # ── Body rendering ───────────────────────────────────────────────────

    def _render_body(self, text):
        lines = text.split('\n')
        parts = []
        para_buf = []
        in_para = False
        para_cls = ''
        in_table = False

        def flush_para():
            nonlocal para_buf, in_para, para_cls
            if para_buf:
                joined = ' '.join(para_buf)
                rendered = self._inline(joined)
                cls = f' class="{para_cls}"' if para_cls else ''
                parts.append(f'<p{cls}>{rendered}</p>')
                para_buf = []
            in_para = False
            para_cls = ''

        def end_table():
            nonlocal in_table
            if in_table:
                parts.append('</table>')
                in_table = False

        i = 0
        while i < len(lines):
            line = lines[i]
            s = line.strip()

            # ── Blank line ──
            if not s:
                flush_para()
                i += 1
                continue

            # ── Block: {title:...} ──
            m = re.match(r'^\{title:(.+?)\}\s*$', s)
            if m:
                flush_para()
                end_table()
                parts.append(f'<h2 class="smcl-title">{self._inline(m.group(1))}</h2>')
                i += 1
                continue

            # ── Block: {marker name} ──
            m = re.match(r'^\{marker\s+(\S+)\}\s*$', s)
            if m:
                self.markers.add(m.group(1))
                parts.append(f'<a id="{_html_esc(m.group(1))}"></a>')
                i += 1
                continue

            # ── Block: paragraph starters ──
            pm = re.match(r'^\{(pstd|phang|phang2|phang3|pmore|pmore2|pmore3|pin|pin2|pin3|psee)\}', s)
            if pm:
                flush_para()
                in_para = True
                para_cls = 'smcl-' + pm.group(1)
                rest = s[pm.end():].strip()
                if rest:
                    para_buf.append(rest)
                i += 1
                continue

            # ── Block: {p # # #} ──
            m = re.match(r'^\{p\s+([\d\s]+)\}', s)
            if m:
                flush_para()
                in_para = True
                para_cls = 'smcl-p'
                rest = s[m.end():].strip()
                if rest:
                    para_buf.append(rest)
                i += 1
                continue

            # ── Block: {p_end} ──
            if s == '{p_end}' or s.startswith('{p_end}'):
                flush_para()
                i += 1
                continue

            # ── Block: {p2colset ...} / {p2colreset} ──
            if re.match(r'^\{p2col(set|reset)\b', s):
                i += 1
                continue

            # ── Block: {p2col:first}second ──
            twocol = self._parse_two_col_line(s, 'p2col')
            if twocol is not None:
                flush_para()
                c1_raw, c2_raw = twocol
                c1 = self._inline(c1_raw)
                if c2_raw.rstrip().endswith('{p_end}'):
                    c2_raw = c2_raw.rstrip()[:-7].strip()
                c2 = self._inline(c2_raw.strip())
                parts.append(f'<div class="smcl-p2col"><span class="smcl-p2col-1">{c1}</span> <span class="smcl-p2col-2">{c2}</span></div>')
                i += 1
                continue

            # ── Block: {p2line} ──
            if s.startswith('{p2line'):
                flush_para()
                parts.append('<hr class="smcl-p2line">')
                i += 1
                continue

            # ── Block: {synoptset ...} ──
            if s.startswith('{synoptset'):
                flush_para()
                # Just setup; actual table starts at synopthdr
                i += 1
                continue

            # ── Block: {synopthdr} or {synopthdr:text} ──
            m = re.match(r'^\{synopthdr(?::(.+?))?\}', s)
            if m:
                flush_para()
                end_table()
                hdr = _html_esc(m.group(1)) if m.group(1) else '<em>Options</em>'
                parts.append('<table class="smcl-synopt-table">')
                parts.append(f'<tr class="smcl-synopt-hdr"><th>{hdr}</th><th>Description</th></tr>')
                in_table = True
                i += 1
                continue

            # ── Block: {synoptline} ──
            if s == '{synoptline}':
                flush_para()
                if in_table:
                    parts.append('<tr class="smcl-synopt-line"><td colspan="2"><hr></td></tr>')
                else:
                    parts.append('<hr class="smcl-hline">')
                i += 1
                continue

            # ── Block: {syntab:text} ──
            m = re.match(r'^\{syntab:(.+?)\}\s*$', s)
            if m:
                flush_para()
                txt = self._inline(m.group(1))
                if in_table:
                    parts.append(f'<tr class="smcl-syntab"><td colspan="2">{txt}</td></tr>')
                else:
                    parts.append(f'<div class="smcl-dlgtab"><strong>{txt}</strong></div>')
                i += 1
                continue

            # ── Block: {synopt :col1}col2 or {synopt:{opt thing}}desc ──
            twocol = self._parse_two_col_line(s, 'synopt')
            if twocol is not None:
                flush_para()
                c1_raw, c2_raw = twocol
                c1 = self._inline(c1_raw)
                # Accumulate continuation lines until {p_end} or blank
                if c2_raw.rstrip().endswith('{p_end}'):
                    c2_raw = c2_raw.rstrip()[:-7].strip()
                else:
                    while i + 1 < len(lines):
                        nxt = lines[i + 1].strip()
                        if not nxt or nxt.startswith('{synopt') or nxt.startswith('{syntab') or nxt == '{synoptline}':
                            break
                        i += 1
                        if nxt == '{p_end}':
                            break
                        if nxt.endswith('{p_end}'):
                            c2_raw += ' ' + nxt[:-7].strip()
                            break
                        c2_raw += ' ' + nxt
                c2 = self._inline(c2_raw.strip())
                if in_table:
                    parts.append(f'<tr class="smcl-synopt-row"><td class="smcl-synopt-col1">{c1}</td><td class="smcl-synopt-col2">{c2}</td></tr>')
                else:
                    parts.append(f'<div class="smcl-synopt"><span class="smcl-synopt-col1">{c1}</span> <span class="smcl-synopt-col2">{c2}</span></div>')
                i += 1
                continue

            # ── Block: {p2coldent:col1}col2 ──
            twocol = self._parse_two_col_line(s, 'p2coldent')
            if twocol is not None:
                flush_para()
                c1_raw, c2_raw = twocol
                c1 = self._inline(c1_raw)
                if c2_raw.rstrip().endswith('{p_end}'):
                    c2_raw = c2_raw.rstrip()[:-7].strip()
                else:
                    while i + 1 < len(lines):
                        nxt = lines[i + 1].strip()
                        if not nxt or nxt.startswith('{synopt') or nxt.startswith('{syntab') or nxt == '{synoptline}' or nxt.startswith('{p2coldent'):
                            break
                        i += 1
                        if nxt == '{p_end}':
                            break
                        if nxt.endswith('{p_end}'):
                            c2_raw += ' ' + nxt[:-7].strip()
                            break
                        c2_raw += ' ' + nxt
                c2 = self._inline(c2_raw.strip())
                if in_table:
                    parts.append(f'<tr class="smcl-synopt-row"><td class="smcl-synopt-col1">{c1}</td><td class="smcl-synopt-col2">{c2}</td></tr>')
                i += 1
                continue

            # ── Block: {dlgtab:text} ──
            m = re.match(r'^\{dlgtab(?:\s[\d\s]*)?:(.+?)\}\s*$', s)
            if m:
                flush_para()
                end_table()
                parts.append(f'<h3 class="smcl-dlgtab">{self._inline(m.group(1))}</h3>')
                i += 1
                continue

            # ── Block: {hline} / {hline #} / {.-} ──
            if re.match(r'^\{hline(?:\s+\d+)?\}\s*$', s) or s == '{.-}':
                flush_para()
                parts.append('<hr class="smcl-hline">')
                i += 1
                continue

            # ── Block: {center:text} / {centre:text} ──
            m = re.match(r'^\{(?:center|centre)(?:\s+\d+)?:(.+?)\}\s*$', s)
            if m:
                flush_para()
                parts.append(f'<div class="smcl-center">{self._inline(m.group(1))}</div>')
                i += 1
                continue

            # ── Block: {right:text} ──
            m = re.match(r'^\{right:(.+?)\}\s*$', s)
            if m:
                flush_para()
                parts.append(f'<div class="smcl-right">{self._inline(m.group(1))}</div>')
                i += 1
                continue

            # ── Default: content line ──
            if in_para:
                if s == '{p_end}':
                    flush_para()
                else:
                    para_buf.append(s)
            else:
                rendered = self._inline(s)
                if rendered.strip():
                    parts.append(f'<div class="smcl-line">{rendered}</div>')

            i += 1

        flush_para()
        end_table()
        return '\n'.join(parts)

    # ── Inline rendering ─────────────────────────────────────────────────

    def _inline(self, text):
        """Render inline SMCL directives within text to HTML."""
        if not text:
            return ''
        out = []
        i = 0
        n = len(text)
        while i < n:
            if text[i] == '{':
                end = _find_brace(text, i)
                if end == -1:
                    out.append(_html_esc('{'))
                    i += 1
                    continue
                tag_html = self._tag(text[i + 1:end])
                out.append(tag_html)
                i = end + 1
            else:
                out.append(_html_esc(text[i]))
                i += 1
        return ''.join(out)

    def _tag(self, content):
        """Render a single SMCL tag (content between { and }) to HTML."""
        name, args, inner = _parse_tag(content)
        lo = name.lower()

        def ri(t):
            return self._inline(t) if t else ''

        # ── Comments / continuation ──
        if name == '*' or name == '...':
            return ''

        # ── Character codes ──
        if lo in ('c', 'char'):
            return _html_esc(_resolve_char(args))

        # ── Font face ──
        if lo == 'bf':
            return f'<strong>{ri(inner)}</strong>' if inner is not None else ''
        if lo == 'it':
            return f'<em>{ri(inner)}</em>' if inner is not None else ''
        if lo == 'sf':
            return ri(inner) if inner is not None else ''
        if lo == 'ul':
            if inner is not None:
                return f'<u>{ri(inner)}</u>'
            return {'on': '<u>', 'off': '</u>'}.get(args.lower(), '')

        # ── Color / style ──
        if lo in ('cmd', 'input', 'inp'):
            return f'<span class="smcl-cmd">{ri(inner)}</span>' if inner is not None else ''
        if lo in ('error', 'err'):
            return f'<span class="smcl-err">{ri(inner)}</span>' if inner is not None else ''
        if lo in ('result', 'res'):
            return f'<span class="smcl-res">{ri(inner)}</span>' if inner is not None else ''
        if lo in ('text', 'txt'):
            return ri(inner) if inner is not None else ''
        if lo == 'com':
            return f'<span class="smcl-com">{ri(inner)}</span>' if inner is not None else ''
        if lo in ('hilite', 'hi'):
            return f'<span class="smcl-hilite">{ri(inner)}</span>' if inner is not None else ''
        if lo == 'reset':
            return ''

        # ── Command abbreviation ──
        if lo == 'cmdab':
            if inner is not None:
                colon_pos = inner.find(':')
                if colon_pos >= 0:
                    min_part = ri(inner[:colon_pos])
                    rest_part = ri(inner[colon_pos + 1:])
                    return f'<span class="smcl-cmd"><u>{min_part}</u>{rest_part}</span>'
                return f'<span class="smcl-cmd">{ri(inner)}</span>'
            return ''

        # ── Options ──
        if lo == 'opt':
            # {opt min:rest} (syntax 4) → abbreviation: min underlined + rest
            if args and inner is not None:
                return f'<span class="smcl-cmd"><u>{_html_esc(args)}</u>{_html_esc(inner)}</span>'
            text_src = inner if inner is not None else args
            if not text_src:
                return ''
            if ':' in text_src:
                colon = text_src.find(':')
                min_p = _html_esc(text_src[:colon])
                rest_p = _html_esc(text_src[colon + 1:])
                return f'<span class="smcl-cmd"><u>{min_p}</u>{rest_p}</span>'
            # Handle opt with parens: opt option(arg)
            return f'<span class="smcl-cmd">{_html_esc(text_src)}</span>'

        if lo == 'opth':
            # {opth min:rest(topic)} — option with abbreviation and help-linked argument
            # When _parse_tag splits on abbreviation colon: args="min", inner="rest(topic)"
            if args and inner is not None:
                raw = args + inner  # Reconstruct full text: "minrest(topic)"
            else:
                raw = args if args else (inner or '')
            # Try to parse option(helpref) pattern
            m = re.match(r'^(\w+)\((.+)\)$', raw)
            if m:
                opt_name = m.group(1)
                helpref = m.group(2)
                # Apply abbreviation underline if args was the min-part
                if args and inner is not None and opt_name.startswith(args):
                    opt_html = f'<u>{_html_esc(args)}</u>{_html_esc(opt_name[len(args):])}'
                else:
                    opt_html = _html_esc(opt_name)
                # helpref may be topic:display or topic##marker:display
                if ':' in helpref:
                    h_topic, h_disp = helpref.split(':', 1)
                    link = self._help_link(h_topic, ri(h_disp))
                    return f'<span class="smcl-cmd">{opt_html}(</span>{link}<span class="smcl-cmd">)</span>'
                return f'<span class="smcl-cmd">{opt_html}({_html_esc(helpref)})</span>'
            # No parens — just render with abbreviation underline if applicable
            if args and inner is not None:
                return f'<span class="smcl-cmd"><u>{_html_esc(args)}</u>{_html_esc(inner)}</span>'
            return f'<span class="smcl-cmd">{_html_esc(raw)}</span>'

        # ── Help links ──
        if lo == 'help':
            return self._help_link(args, ri(inner))
        if lo == 'helpb':
            display = f'<strong>{ri(inner)}</strong>' if inner is not None else f'<strong>{_html_esc(args)}</strong>'
            return self._help_link(args, display)

        # ── Manual help links ──
        if lo == 'manhelp':
            return self._manhelp(args, inner, bold=True)
        if lo == 'manhelpi':
            return self._manhelp(args, inner, bold=False)
        if lo in ('manlink', 'manlinki'):
            parts_a = args.split(None, 1) if args else []
            manual = parts_a[0] if len(parts_a) >= 1 else ''
            entry = parts_a[1] if len(parts_a) >= 2 else ''
            topic_name = entry.replace(' ', '_')
            if lo == 'manlinki':
                disp = f'<em>[{_html_esc(manual)}] {_html_esc(entry)}</em>'
            else:
                disp = f'<strong>[{_html_esc(manual)}] {_html_esc(entry)}</strong>'
            return self._help_link(topic_name, disp)

        if lo == 'mansection':
            display = ri(inner) if inner is not None else _html_esc(args)
            return f'<span class="smcl-mansection">{display}</span>'

        if lo == 'manpage':
            display = ri(inner) if inner is not None else _html_esc(args)
            return f'<span class="smcl-mansection">{display}</span>'

        # ── Browse (external URL) ──
        if lo == 'browse':
            url = args.strip().strip('"')
            display = ri(inner) if inner is not None else _html_esc(url)
            return f'<a class="smcl-browse-link" href="{_html_esc(url)}">{display}</a>'

        # ── Stata command link ──
        if lo in ('stata', 'matacmd'):
            display = ri(inner) if inner is not None else _html_esc(args.strip('"'))
            return f'<span class="smcl-stata-cmd">{display}</span>'

        # ── Special syntax placeholders (with links) ──
        if lo == 'newvar':
            disp = 'newvar' + (ri(inner) if inner is not None else '')
            return f'<a class="smcl-help-link" href="#" data-topic="newvar"><em>{disp}</em></a>'
        if lo in ('var', 'varname'):
            disp = 'varname' + (ri(inner) if inner is not None else '')
            return f'<a class="smcl-help-link" href="#" data-topic="varname"><em>{disp}</em></a>'
        if lo in ('vars', 'varlist'):
            disp = 'varlist' + (ri(inner) if inner is not None else '')
            return f'<a class="smcl-help-link" href="#" data-topic="varlist"><em>{disp}</em></a>'
        if lo == 'depvar':
            disp = 'depvar' + (ri(inner) if inner is not None else '')
            return f'<a class="smcl-help-link" href="#" data-topic="depvar"><em>{disp}</em></a>'
        if lo in ('depvars', 'depvarlist'):
            disp = 'depvarlist' + (ri(inner) if inner is not None else '')
            return f'<a class="smcl-help-link" href="#" data-topic="depvarlist"><em>{disp}</em></a>'
        if lo == 'indepvars':
            disp = 'indepvars' + (ri(inner) if inner is not None else '')
            return f'<a class="smcl-help-link" href="#" data-topic="varlist"><em>{disp}</em></a>'
        if lo == 'ifin':
            return ('[<a class="smcl-help-link" href="#" data-topic="if"><em>if</em></a>]'
                    ' [<a class="smcl-help-link" href="#" data-topic="in"><em>in</em></a>]')
        if lo == 'weight':
            return '[<a class="smcl-help-link" href="#" data-topic="weight"><em>weight</em></a>]'
        if lo == 'dtype':
            return '[<a class="smcl-help-link" href="#" data-topic="data_types"><em>type</em></a>]'

        # ── Layout: inline ──
        if lo == 'hline':
            if args and args.strip().isdigit():
                n = min(int(args.strip()), 120)
                return '\u2500' * n
            return '<hr class="smcl-hline">'
        if lo == '.-':
            return '<hr class="smcl-hline">'
        if lo == 'col':
            try:
                n = int(args)
            except (ValueError, TypeError):
                n = 0
            if n > 0:
                return f'<span style="display:inline-block;min-width:{n}ch"></span>'
            return ''
        if lo == 'space':
            try:
                n = int(args)
            except (ValueError, TypeError):
                n = 1
            return '&nbsp;' * max(n, 0)
        if lo == 'tab':
            return '&nbsp;' * 8
        if lo == 'dup':
            try:
                n = int(args)
            except (ValueError, TypeError):
                n = 0
            return ri(inner) * min(n, 200)
        if lo == 'bind':
            return f'<span style="white-space:nowrap">{ri(inner)}</span>' if inner is not None else ''
        if lo == 'break':
            return '<br>'

        # ── Block-level appearing inline (no-ops) ──
        if lo in ('p_end', 'pstd', 'phang', 'phang2', 'phang3',
                  'pmore', 'pmore2', 'pmore3', 'pin', 'pin2', 'pin3', 'psee',
                  'synoptset', 'synopthdr', 'synoptline', 'syntab',
                  'p2colset', 'p2colreset', 'p2line',
                  'smcl', 'asis'):
            return ''
        if lo == 'p' and (not args or re.match(r'^[\d\s]+$', args)):
            return ''

        # ── Marker ──
        if lo == 'marker':
            self.markers.add(args)
            return f'<a id="{_html_esc(args)}"></a>'

        # ── Title / center / dlgtab appearing inline ──
        if lo == 'title':
            return f'<h2 class="smcl-title">{ri(inner)}</h2>' if inner is not None else ''
        if lo in ('center', 'centre'):
            return f'<div class="smcl-center">{ri(inner)}</div>' if inner is not None else ''
        if lo in ('rcenter', 'rcentre'):
            return f'<div class="smcl-center">{ri(inner)}</div>' if inner is not None else ''
        if lo == 'right':
            return f'<div class="smcl-right">{ri(inner)}</div>' if inner is not None else ''
        if lo == 'dlgtab':
            return f'<h3 class="smcl-dlgtab">{ri(inner)}</h3>' if inner is not None else ''

        # ── Misc display links (render text only) ──
        if lo in ('help_d', 'search_d', 'view_d', 'net_d', 'netfrom_d',
                  'ado_d', 'update_d', 'back', 'clearmore'):
            return ri(inner) if inner is not None else ''
        if lo in ('search', 'dialog', 'view', 'net', 'ado', 'update'):
            return ri(inner) if inner is not None else _html_esc(args)

        if lo == 'ccl':
            return f'<span class="smcl-res">{_html_esc(args)}</span>'

        # ── Synopt/p2col inline fallthrough ──
        if lo in ('synopt', 'p2col', 'p2coldent'):
            return ri(inner) if inner is not None else ''

        # ── Metadata tags (already extracted) ──
        if lo in ('viewerjumpto', 'vieweralsosee', 'viewerdialog', 'findalias'):
            return ''

        # ── Unknown tag: render inner or args ──
        if inner is not None:
            return ri(inner)
        if args:
            return _html_esc(args)
        return ''

    # ── Link helpers ─────────────────────────────────────────────────────

    def _help_link(self, topic_str, display=''):
        """Build an <a> tag for a help topic, handling ##marker syntax."""
        topic_str = topic_str.strip().strip('"')
        if not topic_str and not display:
            return ''

        base_topic = topic_str
        marker = ''
        if '##' in topic_str:
            parts = topic_str.split('##', 1)
            base_topic = parts[0]
            marker = parts[1]
            # Strip |viewername from marker
            if '|' in marker:
                marker = marker.split('|')[0]

        if not display:
            if base_topic and marker:
                display = _html_esc(f'{base_topic}##{marker}')
            elif base_topic:
                display = _html_esc(base_topic)
            else:
                display = _html_esc(marker)

        if base_topic:
            attrs = f'data-topic="{_html_esc(base_topic)}"'
            if marker:
                attrs += f' data-marker="{_html_esc(marker)}"'
            return f'<a class="smcl-help-link" href="#" {attrs}>{display}</a>'
        else:
            # Same-page anchor jump
            return f'<a class="smcl-help-link" href="#{_html_esc(marker)}">{display}</a>'

    def _manhelp(self, args, inner, bold=True):
        parts_a = args.split() if args else []
        topic_name = parts_a[0] if len(parts_a) >= 1 else ''
        manual = parts_a[1] if len(parts_a) >= 2 else ''
        # Handle MANUAL:display format (e.g., "BAYES:bayes: regress")
        if ':' in manual and inner is None:
            # The rest after first space and first manual word could be display
            pass
        display = self._inline(inner) if inner is not None else ''
        if not display:
            tag = 'strong' if bold else 'em'
            display = f'<{tag}>[{_html_esc(manual)}] {_html_esc(topic_name)}</{tag}>'
        return self._help_link(topic_name, display)

    # ── HTML wrapping ────────────────────────────────────────────────────

    def _wrap_html(self, body, topic):
        toc_html = self._build_toc()
        also_html = self._build_alsosee()
        nav_html = ('<div class="smcl-nav-bar">'
                    '<button data-action="helpBack" title="Back">&#x2190; Back</button>'
                    '<button data-action="helpForward" title="Forward">&#x2192; Forward</button>'
                    f'<span class="smcl-nav-topic">{_html_esc(topic)}</span>'
                    '</div>')

        return f'''<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta http-equiv="Content-Security-Policy"
      content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline';">
<title>Stata Help: {_html_esc(topic)}</title>
<style>{_CSS}</style>
</head>
<body>
{nav_html}
{toc_html}
{body}
{also_html}
<script>{_JS}</script>
</body>
</html>'''

    def _build_toc(self):
        if not self.toc:
            return ''
        items = []
        for text, target in self.toc:
            if '##' in target:
                marker = target.split('##', 1)[1]
                items.append(f'<a class="smcl-help-link" href="#{_html_esc(marker)}">{_html_esc(text)}</a>')
            else:
                items.append(_html_esc(text))
        return '<nav class="smcl-toc">' + ' &nbsp;|&nbsp; '.join(items) + '</nav>'

    def _build_alsosee(self):
        if not self.also_see:
            return ''
        items = []
        for text, target in self.also_see:
            if text == '---':
                items.append('<span class="smcl-alsosee-sep">|</span>')
            elif target:
                topic = target.replace(' ', '_')
                items.append(f'<a class="smcl-help-link" href="#" data-topic="{_html_esc(topic)}">{_html_esc(text)}</a>')
            elif text:
                items.append(f'<span>{_html_esc(text)}</span>')
        if not items:
            return ''
        return '<div class="smcl-alsosee"><strong>Also see:</strong> ' + '  '.join(items) + '</div>'


# ── Public convenience function ──────────────────────────────────────────

def smcl_to_html(smcl_text, include_resolver=None, topic=''):
    """Convert raw SMCL text to a complete HTML page."""
    parser = SmclParser()
    return parser.convert(smcl_text, include_resolver=include_resolver, topic=topic)
