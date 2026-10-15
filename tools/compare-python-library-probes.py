#!/usr/bin/env python3
"""Print comparable familiar-library outcomes without interpreting historical wire formats."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'sdk/python/src'))
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrReport


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    args = parser.parse_args()
    before, after = (json.loads(path.read_text()) for path in (args.before, args.after))
    assert set(after['snapshots']) == {'toolz', 'funcy', 'pydash'}
    assert before['snapshots'] == after['snapshots'], 'compare identical familiar source snapshots'
    print('Previous completion counts:', before['declaration_summary'])
    print('Current completion counts:', after['declaration_summary'])
    for helper in ('toolz-return-none', 'funcy-inc', 'pydash-add'):
        def report(data):
            row = next(row for row in data['declaration_cases'] if row['helper'] == helper
                       and row['context'] == 'full-package' and row['variant'] == 'original'
                       and row['case'] == 'positive')
            return row['invocations'][1]['report']
        old, new = report(before), report(after)
        old_files, new_files = (row['inputs']['modules']['files'] for row in (old, new))
        assert all(old_files[path] == new_files[path] for path in old_files.keys() & new_files.keys())
        print(helper, 'unchecked statements:',
              *(len(row['initialization']['sites']) + row['initialization']['omitted'] for row in (old, new)),
              '; file counts:', len(old_files), len(new_files),
              '; added files:', sorted(new_files.keys() - old_files.keys()),
              '; dependency cutoffs:', old['inputs']['modules']['cutoffs'], new['inputs']['modules']['cutoffs'])
        if helper == 'funcy-inc':
            lookups = new['inputs']['modules']['lookups']
            dependencies = FlowDependencies.from_report(FrReport(new, ()))
            print('Funcy searches:', dependencies.lookup_count)
            print('Funcy initializer searches:', sum(row['importer'] == 'funcy/__init__.py' for row in lookups))
            print('Funcy EMPTY bindings:', [{'importer': row.importer, 'alias': row.alias,
                  'resolution': row.resolution, 'declarations': [(hop.path, hop.name) for hop in row.binding_chain]}
                  for row in dependencies.lookups if row.member == 'EMPTY'])
            def sites(data):
                return {(site['occurrence']['path'], site['occurrence']['location']['range']['start']['line'],
                         tuple(site['reasons'])) for site in data['initialization']['sites']}
            print('Funcy removed unchecked sites:', sorted(sites(old) - sites(new)))
            print('Funcy added unchecked sites:', sorted(sites(new) - sites(old)))


if __name__ == '__main__':
    main()
