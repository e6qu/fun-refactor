"""Exercise public-site links without fetching external resources or building WASM."""
import importlib.util
from pathlib import Path
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
        review.ROOT = Path(self.directory.name).resolve()
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
        self.page('index.html', '<a href="../../outside.html">Outside</a>')
        with self.assertRaisesRegex(ValueError, 'missing ../../outside.html'):
            review.check_website_links()

    def test_only_the_declared_browser_build_directory_can_be_absent(self):
        self.page('index.html', '<a href="playground/">Built separately</a>')
        review.check_website_links()
        self.page('index.html', '<a href="another-build/">Missing</a>')
        with self.assertRaisesRegex(ValueError, 'missing another-build/'):
            review.check_website_links()


if __name__ == '__main__':
    unittest.main()
