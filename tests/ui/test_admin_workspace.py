import re
import subprocess
import tempfile
import unittest
from pathlib import Path


ADMIN_HTML = Path("src/web/admin.html")


class AdminWorkspaceTest(unittest.TestCase):
    maxDiff = None

    def load_html(self) -> str:
        return ADMIN_HTML.read_text(encoding="utf-8")

    def test_workspace_shell_exists(self) -> None:
        html = self.load_html()
        self.assertIn('id="proxy-workspace"', html)
        self.assertIn('id="proxy-workspace-header"', html)
        self.assertIn('id="proxy-toolbar"', html)
        self.assertIn('id="proxy-toolbar-actions"', html)
        self.assertIn('id="proxy-toolbar-meta"', html)

    def test_legacy_sections_are_removed(self) -> None:
        html = self.load_html()
        self.assertNotIn('section-title">操作<', html)
        self.assertNotIn('section-title">类型分布<', html)

    def test_workspace_section_follows_subscriptions(self) -> None:
        html = self.load_html()
        self.assertLess(html.index('section-title">订阅源<'), html.index('id="proxy-workspace"'))

    def test_workspace_moves_type_chips_into_header(self) -> None:
        html = self.load_html()
        self.assertIn('id="workspace-type-chips"', html)
        self.assertIn('id="proxy-filter-bar"', html)

    def test_inline_script_has_valid_js_syntax(self) -> None:
        html = self.load_html()
        match = re.search(r"<script>(.*)</script>", html, re.S)
        self.assertIsNotNone(match)

        with tempfile.NamedTemporaryFile("w", suffix=".js", delete=False, encoding="utf-8") as handle:
            handle.write(match.group(1))
            script_path = Path(handle.name)

        try:
            result = subprocess.run(
                ["node", "--check", str(script_path)],
                capture_output=True,
                text=True,
                check=False,
            )
        finally:
            script_path.unlink(missing_ok=True)

        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
