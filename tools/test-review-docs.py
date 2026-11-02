"""Exercise public-site links without fetching external resources or building WASM."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('review_docs', Path(__file__).with_name('review-docs.py'))
review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)


class WebsiteLinks(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        previous = review.ROOT
        self.addCleanup(setattr, review, 'ROOT', previous)
        review.ROOT = Path(self.directory.name).resolve() / 'repository'
        review.ROOT.mkdir()
        self.docs = review.ROOT / 'docs'
        self.docs.mkdir()

    def page(self, name, text):
        (self.docs / name).write_text(text)

    def test_cross_page_anchors_encoded_assets_and_external_links(self):
        self.page('index.html', '<a href="answer.html#résultat">Read</a>'
                  '<img src="a%20b.svg"/><a href="https://example.invalid/missing">External</a>')
        self.page('answer.html', '<section id="résultat"></section><a name="older"></a>'
                  '<a href="#older">Local</a>')
        (self.docs / 'a b.svg').write_text('<svg/>')
        review.check_website_links()

    def test_missing_anchor_fails_even_when_page_exists(self):
        self.page('index.html', '<a href="answer.html#missing">Read</a>')
        self.page('answer.html', '<section id="answer"></section>')
        with self.assertRaisesRegex(ValueError, 'missing heading answer.html#missing'):
            review.check_website_links()

    def test_missing_asset_fails(self):
        self.page('index.html', '<script src="missing.js"></script>')
        with self.assertRaisesRegex(ValueError, 'missing missing.js'):
            review.check_website_links()

    def test_generated_article_anchor_uses_the_renderers_data(self):
        self.page('index.html', '<a href="translate.html#route">Read</a>')
        self.page('translate.html', '<div id="cases"></div>')
        data = self.docs / 'translate-data.js'
        data.write_text('export const TRANSLATIONS = [{\n  id: "route",\n}];\n')
        review.check_website_links()
        data.write_text('export const TRANSLATIONS = [{\n  id: "different",\n}];\n')
        with self.assertRaisesRegex(ValueError, 'missing heading translate.html#route'):
            review.check_website_links()

    def test_links_cannot_escape_the_repository(self):
        (review.ROOT.parent / 'outside.html').write_text('<p>Existing outside file</p>')
        self.page('index.html', '<a href="../../outside.html">Outside</a>')
        with self.assertRaisesRegex(ValueError, 'missing ../../outside.html'):
            review.check_website_links()

    def test_only_the_declared_browser_build_directory_can_be_absent(self):
        self.page('index.html', '<a href="playground/">Built separately</a>')
        review.check_website_links()
        self.page('index.html', '<a href="another-build/">Missing</a>')
        with self.assertRaisesRegex(ValueError, 'missing another-build/'):
            review.check_website_links()

    def test_sparse_checkout_selects_new_linked_evidence_before_it_exists(self):
        page = review.ROOT / 'README.md'
        page.write_text('[Fresh result](tests/agent-eval/results/new/manifest.json)\n'
                        '[Same result](tests/agent-eval/results/new/manifest.json#part)\n'
                        '[External](https://example.invalid/tests/agent-eval/results/other.json)\n'
                        '``[Inline](tests/agent-eval/results/inline.json)``\n'
                        '```sh\n[Example](tests/agent-eval/results/example.json)\n```\n')
        self.assertEqual(review.evidence_links([page]),
                         [Path('tests/agent-eval/results/new/manifest.json')])

    def test_sparse_checkout_resolves_nested_encoded_links_and_directories(self):
        page = self.docs / 'results.md'
        page.write_text('[Report](../tests/agent-eval/results/a%20b/result.json)\n'
                        '[Directory](../tests/agent-eval/results/another/)\n')
        self.assertEqual(review.evidence_links([page]),
                         [Path('tests/agent-eval/results/a b/result.json'),
                          Path('tests/agent-eval/results/another')])

    def test_sparse_checkout_refuses_escaping_or_multiline_paths(self):
        page = self.docs / 'results.md'
        for target in ['../../outside.json', '../tests/agent-eval/results/new%0Aother.json']:
            page.write_text(f'[Invalid]({target})\n')
            with self.subTest(target=target), self.assertRaises(ValueError):
                review.evidence_links([page])

    def test_linked_manifest_is_restored_without_its_source_archive(self):
        root = review.ROOT
        (root / 'README.md').write_text('[New](tests/agent-eval/results/new/manifest.json)\n')
        record = root / 'tests/agent-eval/results/new'
        record.mkdir(parents=True)
        (record / 'manifest.json').write_text('{}\n')
        (record / 'source.tar.gz').write_bytes(b'Unneeded archive control')
        def git(directory, *arguments, input):
            return subprocess.run(['git', '-C', str(directory), *arguments], input=input,
                                  text=True, capture_output=True, check=True)
        git(root, 'init', '-b', 'main', input=None)
        git(root, 'add', '.', input=None)
        git(root, '-c', 'user.name=Fr test', '-c', 'user.email=fr-test@example.invalid',
            'commit', '-m', 'Prepare the sparse evidence checkout control', input=None)
        checkout = root.parent / 'checkout'
        subprocess.run(['git', 'clone', '--no-checkout', '--no-hardlinks', str(root), str(checkout)],
                       capture_output=True, text=True, check=True)
        git(checkout, 'sparse-checkout', 'set', '--no-cone', '--stdin',
            input='/*\n!/tests/agent-eval/results/*\n')
        git(checkout, 'checkout', 'main', input=None)
        manifest = checkout / 'tests/agent-eval/results/new/manifest.json'
        self.assertFalse(manifest.exists())
        review.ROOT = checkout
        patterns = ''.join('/' + p.as_posix() + '\n' for p in review.evidence_links([checkout / 'README.md']))
        git(checkout, 'sparse-checkout', 'add', '--stdin', input=patterns)
        self.assertTrue(manifest.is_file())
        self.assertFalse(manifest.with_name('source.tar.gz').exists())


if __name__ == '__main__':
    unittest.main()
