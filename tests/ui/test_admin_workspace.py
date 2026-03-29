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

    def proxy_table_header_html(self) -> str:
        html = self.load_html()
        match = re.search(
            r'id="proxy-workspace".*?<thead><tr>(.*?)</tr></thead>\s*<tbody id="proxy-table">',
            html,
            re.S,
        )
        self.assertIsNotNone(match)
        return match.group(1)

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

    def test_workspace_scripts_define_header_and_toolbar_renderers(self) -> None:
        html = self.load_html()
        self.assertIn("function renderWorkspaceHeader(", html)
        self.assertIn("function renderWorkspaceToolbar(", html)
        self.assertIn("当前筛选", html)
        self.assertIn("已选", html)

    def test_proxy_table_headers_match_workspace_design(self) -> None:
        header_html = self.proxy_table_header_html()
        self.assertIn(">节点信息<", header_html)
        self.assertIn(">端口 / 错误<", header_html)
        self.assertIn(">质量标签<", header_html)
        self.assertNotIn('sortBy(\'risk\')', header_html)
        self.assertNotIn(">类型<", header_html)
        self.assertNotIn(">服务器<", header_html)
        self.assertNotIn(">IP<", header_html)
        self.assertNotIn(">IP族<", header_html)
        self.assertNotIn(">国家<", header_html)
        self.assertNotIn(">GPT<", header_html)
        self.assertNotIn(">Google<", header_html)
        self.assertNotIn(">住宅<", header_html)

    def test_workspace_uses_more_actions_and_unknown_state_labels(self) -> None:
        html = self.load_html()
        self.assertIn("更多", html)
        self.assertIn("未质检", html)
        self.assertIn("IP族未知", html)
        self.assertNotIn("等待质量数据", html)

    def test_toolbar_only_sticks_in_selected_mode(self) -> None:
        html = self.load_html()
        self.assertIn(".workspace-toolbar.is-selected", html)
        self.assertIn("toolbar.classList.toggle('is-selected', selectionCount > 0)", html)

    def test_row_actions_menu_is_not_absolute_popover(self) -> None:
        html = self.load_html()
        self.assertIn(".row-actions-popover { display:flex;", html)
        self.assertNotIn("top:calc(100% + 6px)", html)
        self.assertNotIn("right:0;", html)

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
