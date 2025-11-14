#!/usr/bin/env python3

import re, json, os, argparse

def parse_lcov(path):
    files = []
    cur = None
    with open(path, 'r', encoding='utf-8') as f:
        for line in f:
            line = line.strip()
            if line.startswith('TN:'):  # test name, optional
                continue
            if line.startswith('SF:'):
                if cur: files.append(cur)
                cur = {'file': line[3:], 'lines': {}, 'functions': {}, 'branches': []}
            elif line.startswith('DA:'):
                parts = line[3:].split(',', 2)
                ln, hits = parts[0], parts[1]
                cur['lines'][int(ln)] = int(hits)
            elif line.startswith('FN:'):
                s = line[3:]
                fn_line_str, fn_name = s.split(',', 1)
                fn_line = int(fn_line_str)
                cur.setdefault('fn_defs', {})[fn_name] = fn_line
            elif line.startswith('FNDA:'):
                hits, fn_name = line[5:].split(',', 1)
                cur['functions'][fn_name] = int(hits)
            elif line.startswith('BRDA:'):
                l, b, br, hits = line[5:].split(',')
                cur['branches'].append({
                    'line': int(l), 'block': int(b), 'branch': int(br),
                    'hits': 0 if hits in ('0', '-') else int(hits)
                })
            elif line == 'end_of_record':
                if cur: files.append(cur); cur = None
    if cur: files.append(cur)
    return files

def merge_ranges(lines):
    ranges = []
    for ln in sorted(lines):
        if not ranges or ln > ranges[-1][1] + 1:
            ranges.append([ln, ln])
        else:
            ranges[-1][1] = ln
    return ranges

def llm_payload_from_lcov(lcov_path, repo_root, context_lines=40):
    files = parse_lcov(lcov_path)
    out = []
    for f in files:
        uncovered_lines = [ln for ln, hits in f['lines'].items() if hits == 0]
        uncovered_ranges = merge_ranges(uncovered_lines)

        unexec_funcs = []
        for name, hits in f['functions'].items():
            if hits == 0:
                unexec_funcs.append({'name': name, 'line': f.get('fn_defs', {}).get(name)})

        uncovered_branches = []
        for br in f['branches']:
            if br['hits'] == 0:
                uncovered_branches.append({'line': br['line'], 'branch': br['branch']})

        snippets = []
        src_path = os.path.join(repo_root, f['file'])
        code = ''
        try:
            with open(src_path, 'r', encoding='utf-8') as sf:
                code = sf.readlines()
        except Exception:
            pass

        def slice_around(start, end):
            if not code: return ""
            lo = max(0, start - 1 - context_lines)
            hi = min(len(code), end + context_lines)
            return ''.join(code[lo:hi])

        for r in uncovered_ranges:
            snippets.append({'type': 'lines', 'range': r, 'code': slice_around(r[0], r[1])})
        for uf in unexec_funcs:
            if uf['line']:
                snippets.append({'type': 'function', 'name': uf['name'], 'line': uf['line'],
                                 'code': slice_around(uf['line'], uf['line'] + 1)})

        out.append({
            'file': f['file'],
            'uncovered_ranges': uncovered_ranges,
            'uncovered_branches': uncovered_branches,
            'unexecuted_functions': unexec_funcs,
            'snippets': snippets
        })
    return out

# Usage:
if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='LCOV → AI payload preview')
    parser.add_argument(
        '--size',
        type=int,
        default=20000,
        help='Max bytes to print from JSON preview (default: 20000)',
    )
    parser.add_argument(
        '--raw',
        action='store_true',
        help='Print raw JSON only without prompt header',
    )
    args = parser.parse_args()
    payload = llm_payload_from_lcov('build/coverage/lcov.info', repo_root='.')
    if not args.raw:
        try:
            with open('prompts/TESTS.md', 'r', encoding='utf-8') as pf:
                print(pf.read())
        except Exception as e:
            print(f'# Warning: failed to read prompt: {e}')
    print(json.dumps(payload[:10], ensure_ascii=False)[:args.size])
