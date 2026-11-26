"""Tests of scripts/check-ci-config.py, which check that the project passes and that the checker finds each problem."""

import importlib.util
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("check_ci_config", ROOT / "scripts" / "check-ci-config.py")
check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check)


class CheckCiConfig(unittest.TestCase):
    """Each check runs on a copy of the project's own files, with one change at a time."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        for name in (".config/nextest.toml", ".github/workflows/check.yml", ".github/workflows/release.yml",
                     ".github/workflows/docs-publish.yml", "Cargo.toml"):
            (self.root / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / name, self.root / name)
        (self.root / "tests").mkdir()
        (self.root / "tests" / "conformance.rs").write_text("#[test]\nfn plan_cli_and_evidence() {}\n")
        (self.root / "src").mkdir()
        (self.root / "src" / "lib.rs").write_text("fn helper() {}\n")

    def edit(self, name, old, new):
        """Replace OLD with NEW once in the copied file NAME."""
        path = self.root / name
        text = path.read_text()
        self.assertIn(old, text)
        path.write_text(text.replace(old, new, 1))

    def problems(self):
        return check.nextest_problems(self.root) + check.workflow_problems(self.root)

    def test_the_project_passes(self):
        self.assertEqual(check.main(["check", str(ROOT)]), 0)

    def test_the_copy_passes(self):
        self.assertEqual(self.problems(), [])

    def test_a_renamed_test_is_found(self):
        (self.root / "tests" / "conformance.rs").write_text("#[test]\nfn renamed() {}\n")
        self.assertEqual(len(self.problems()), 1)
        self.assertIn("test(plan_cli_and_evidence)", self.problems()[0])

    def test_a_missing_binary_is_found(self):
        (self.root / "tests" / "conformance.rs").unlink()
        (self.root / "tests" / "other.rs").write_text("fn plan_cli_and_evidence() {}\n")
        self.assertTrue(any("binary(conformance)" in p for p in self.problems()))

    def test_a_group_whose_tests_do_not_run_alone_is_found(self):
        self.edit(".config/nextest.toml", "threads-required = 'num-test-threads'\n", "")
        self.assertTrue(any("threads-required" in p for p in self.problems()))

    def test_a_script_fetched_by_tag_is_found(self):
        self.edit(".github/workflows/release.yml", "rahuldave/jagt/604e3fb7754a168a28beeaa908908abd8589eee1/",
                  "rahuldave/jagt/v0.6.4/")
        self.has("rahuldave/jagt/v0.6.4/")

    def test_a_kept_coverage_stamp_is_found(self):
        self.edit(".github/workflows/release.yml", check.STAMP, "true")
        self.assertTrue(any("coverage stamp" in p for p in self.problems()))

    def test_a_stamp_removed_after_just_is_found(self):
        path = self.root / ".github/workflows/release.yml"
        text = path.read_text().replace(f"run: {check.STAMP}", "run: true", 1)
        path.write_text(text.replace("run: just check", f"run: just check && {check.STAMP}", 1))
        self.assertTrue(any("coverage stamp" in p for p in self.problems()))

    def test_a_cache_key_with_no_image_is_found(self):
        self.edit(".github/workflows/check.yml", "${{ steps.rust.outputs.image }}-", "")
        self.assertTrue(any("holds no image" in p for p in self.problems()))

    def test_main_exits_with_1_on_a_problem(self):
        (self.root / "tests" / "conformance.rs").write_text("#[test]\nfn renamed() {}\n")
        self.assertEqual(check.main(["check", str(self.root)]), 1)

    def test_main_exits_with_1_when_the_config_is_missing(self):
        (self.root / ".config" / "nextest.toml").unlink()
        self.assertEqual(check.main(["check", str(self.root)]), 1)

    def test_a_deleted_isolation_is_found(self):
        path = self.root / ".config" / "nextest.toml"
        text = path.read_text()
        path.write_text(text[:text.index("[[profile.default.overrides]]")])
        self.assertTrue(any('exactly one override' in p for p in self.problems()))
    def test_an_override_with_no_group_and_no_threads_is_found(self):
        self.edit(".config/nextest.toml", "test-group = 'serial'\nthreads-required = 'num-test-threads'\n", "")
        self.assertTrue(any('needs threads-required = "num-test-threads"' in p for p in self.problems()))
    def test_a_regex_term_is_found(self):
        self.edit(".config/nextest.toml", "test(plan_cli_and_evidence)", "test(/plan_cli_and_evidenze/)")
        self.assertTrue(any("not a plain name" in p for p in self.problems()))

    def test_a_restore_key_with_no_image_is_found(self):
        path = self.root / ".github/workflows/check.yml"
        lines = path.read_text().splitlines(keepends=True)
        index = next(i for i, line in enumerate(lines) if "restore-keys" in line) + 1
        lines[index] = lines[index].replace("${{ steps.rust.outputs.image }}-", "")
        path.write_text("".join(lines))
        self.assertTrue(any("holds no image" in p for p in self.problems()))

    def test_an_image_that_is_not_written_is_found(self):
        self.edit(".github/workflows/release.yml", check.IMAGE_OUTPUT, "true")
        self.assertTrue(any('no step rust' in p for p in self.problems()))
    def test_a_stamp_command_kept_only_in_a_comment_is_found(self):
        self.edit(".github/workflows/release.yml", f"run: {check.STAMP}", f"run: true\n        # was: {check.STAMP}")
        self.assertTrue(any("coverage stamp" in p for p in self.problems()))

    def test_a_stamp_step_with_a_condition_is_found(self):
        self.edit(".github/workflows/release.yml", f"        run: {check.STAMP}", f"        if: false\n        run: {check.STAMP}")
        self.assertTrue(any("coverage stamp" in p for p in self.problems()))

    def test_a_multiline_just_before_the_stamp_is_found(self):
        self.edit(".github/workflows/release.yml", "      - name: Drop the coverage stamp",
                  "      - name: Early\n        run: |\n          echo hi\n          just setup\n      - name: Drop the coverage stamp")
        self.assertTrue(any("coverage stamp" in p for p in self.problems()))

    def test_a_commented_just_before_the_stamp_is_not_a_just(self):
        self.edit(".github/workflows/release.yml", "      - name: Drop the coverage stamp",
                  "      # - run: just setup\n      - name: Drop the coverage stamp")
        self.assertEqual(self.problems(), [])

    def test_a_filter_with_not_or_and_is_found(self):
        for joined in ("not test(plan_cli_and_evidence) & not binary(conformance)",
                       "test(plan_cli_and_evidence) & binary(conformance)"):
            with self.subTest(joined=joined):
                self.setUp()
                self.edit(".config/nextest.toml", "test(plan_cli_and_evidence) | binary(conformance)", joined)
                self.assertTrue(any("joins its terms" in p for p in self.problems()))
    def test_an_override_of_another_profile_is_found(self):
        self.edit(".config/nextest.toml", "[[profile.default.overrides]]", "[[profile.unused.overrides]]")
        self.assertTrue(any('exactly one override' in p for p in self.problems()))
    def test_a_second_override_is_found(self):
        path = self.root / ".config" / "nextest.toml"
        path.write_text(path.read_text() + "\n[[profile.default.overrides]]\nfilter = 'binary(conformance)'\n"
                        "test-group = 'serial'\n")
        self.assertTrue(any('exactly one override' in p for p in self.problems()))
    def test_a_glob_term_with_a_known_prefix_is_found(self):
        self.edit(".config/nextest.toml", "test(plan_cli_and_evidence)", "test(plan_cli_and_evidence*)")
        self.assertTrue(any("not a plain name" in p for p in self.problems()))

    def test_an_image_written_only_by_a_dead_step_is_found(self):
        self.edit(".github/workflows/check.yml", check.IMAGE_OUTPUT, 'echo "image=" >> "$GITHUB_OUTPUT"')
        self.edit(".github/workflows/check.yml", "      - name: Restore the Rust cache",
                  f"      - name: Dead\n        if: false\n        run: |\n          {check.IMAGE_OUTPUT}\n"
                  "      - name: Restore the Rust cache")
        self.assertTrue(any('no step rust' in p for p in self.problems()))
    def test_a_key_that_names_no_toolchain_is_found(self):
        path = self.root / ".github/workflows/check.yml"
        lines = path.read_text().splitlines(keepends=True)
        index = next(i for i, line in enumerate(lines) if line.strip().startswith("key:"))
        lines[index] = "          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}\n"
        path.write_text("".join(lines))
        self.assertTrue(any("holds no image" in p for p in self.problems()))

    def test_a_bare_just_before_the_stamp_is_found(self):
        self.edit(".github/workflows/release.yml", "      - name: Drop the coverage stamp",
                  "      - name: Bare\n        run: just\n      - name: Drop the coverage stamp")
        self.assertTrue(any("coverage stamp" in p for p in self.problems()))

    def test_a_stamp_after_just_in_one_step_is_found(self):
        self.edit(".github/workflows/release.yml", f"        run: {check.STAMP}",
                  f"        run: |\n          just check\n          {check.STAMP}")
        self.assertTrue(any("coverage stamp" in p for p in self.problems()))


    def test_a_rust_step_with_a_condition_is_found(self):
        self.edit(".github/workflows/check.yml", "        id: rust\n", "        id: rust\n        if: false\n")
        self.assertTrue(any('no step rust' in p for p in self.problems()))

    def test_an_earlier_override_that_lowers_the_threads_is_found(self):
        self.edit(".config/nextest.toml", "[[profile.default.overrides]]",
                  "[[profile.default.overrides]]\nfilter = 'binary(conformance)'\nthreads-required = 1\n\n"
                  "[[profile.default.overrides]]")
        self.assertTrue(any('exactly one override' in p for p in self.problems()))

    def test_an_override_with_a_platform_is_found(self):
        self.edit(".config/nextest.toml", "test-group = 'serial'\n", "test-group = 'serial'\nplatform = 'cfg(windows)'\n")
        self.assertTrue(any('holds platform' in p for p in self.problems()))

    def test_a_cache_step_before_the_rust_step_is_found(self):
        path = self.root / ".github/workflows/check.yml"
        text = path.read_text()
        start = text.index("      - name: Restore the Rust cache")
        end = text.index("      - name: Verify complete vendored provider")
        cache = text[start:end]
        text = text[:start] + text[end:]
        rust = text.index("      - name: Install the Rust toolchain of rust-toolchain.toml")
        path.write_text(text[:rust] + cache + text[rust:])
        self.assertTrue(any('no step rust' in p for p in self.problems()))

    def test_a_misspelt_image_output_is_found(self):
        self.edit(".github/workflows/check.yml", "${{ steps.rust.outputs.image }}-${{ runner.arch }}-rust-${{ steps.rust.outputs.toolchain }}-check-${{ hashFiles('**/Cargo.lock') }}-${{ github.sha }}",
                  "${{ steps.rust.outputs.imageos }}-${{ runner.arch }}-rust-${{ steps.rust.outputs.toolchain }}-check-${{ hashFiles('**/Cargo.lock') }}-${{ github.sha }}")
        self.assertTrue(any('holds no image' in p for p in self.problems()))

    def test_a_test_name_kept_only_in_a_comment_is_found(self):
        (self.root / "tests" / "conformance.rs").write_text("// fn plan_cli_and_evidence() was here\n#[test]\nfn renamed() {}\n")
        self.assertTrue(any('test(plan_cli_and_evidence) names no test' in p for p in self.problems()))

    def test_a_function_that_is_no_test_is_found(self):
        self.edit(".config/nextest.toml", "test(plan_cli_and_evidence) |", "test(plan_cli_and_evidence) | test(helper) |")
        self.assertTrue(any('test(helper) names no test' in p for p in self.problems()))

    def test_a_full_path_term_is_found(self):
        self.edit(".config/nextest.toml", "test(plan_cli_and_evidence)", "test(=main)")
        self.assertTrue(any('not a plain name' in p for p in self.problems()))

    def test_a_one_line_restore_key_is_found(self):
        path = self.root / ".github/workflows/check.yml"
        text = path.read_text()
        start = text.index("          restore-keys: |")
        end = text.index("      - name: Verify complete vendored provider")
        path.write_text(text[:start] + "          restore-keys: ${{ runner.os }}-rust-\n" + text[end:])
        self.assertTrue(any('restore-keys must be a block' in p for p in self.problems()))

    def test_a_restore_by_actions_cache_with_no_stamp_step_is_found(self):
        self.edit(".github/workflows/release.yml", "uses: actions/cache/restore@v4", "uses: actions/cache@v4")
        self.edit(".github/workflows/release.yml", f"        run: {check.STAMP}", "        run: true")
        self.assertTrue(any("has no step of only a name and run:" in p for p in self.problems()))

    def test_a_stamp_step_in_another_folder_is_found(self):
        self.edit(".github/workflows/release.yml", f"        run: {check.STAMP}",
                  f"        working-directory: docs\n        run: {check.STAMP}")
        self.assertTrue(any('coverage stamp' in p for p in self.problems()))

    def has(self, text):
        """Assert that the copy has a problem that holds TEXT."""
        problems = self.problems()
        self.assertTrue(any(text in p for p in problems), problems)

    def test_an_override_that_drops_a_term_that_must_run_alone_is_found(self):
        self.edit(".config/nextest.toml", " | binary(conformance)", "")
        self.has("does not name binary(conformance)")

    def test_a_function_with_another_attribute_is_found(self):
        (self.root / "src" / "lib.rs").write_text("#[inline]\nfn helper() {}\n")
        self.edit(".config/nextest.toml", "test(plan_cli_and_evidence) |", "test(plan_cli_and_evidence) | test(helper) |")
        self.has("test(helper) names no test")

    def test_a_function_after_a_test_is_not_a_test(self):
        (self.root / "src" / "lib.rs").write_text("#[test]\nfn a() {}\nfn helper() {}\n")
        self.edit(".config/nextest.toml", "test(plan_cli_and_evidence) |", "test(plan_cli_and_evidence) | test(helper) |")
        self.has("test(helper) names no test")

    def test_a_test_kept_only_in_a_block_comment_is_found(self):
        (self.root / "tests" / "conformance.rs").write_text("/*\n#[test]\nfn plan_cli_and_evidence() {}\n*/\n")
        self.has("test(plan_cli_and_evidence) names no test")

    def test_a_rust_step_of_another_job_is_found(self):
        path = self.root / ".github/workflows/check.yml"
        text = path.read_text()
        text = text.replace("        id: rust\n", "        id: rust-elsewhere\n", 1)
        path.write_text(text.replace("jobs:\n", (
            "jobs:\n  other:\n    runs-on: ubuntu-24.04\n    steps:\n      - name: Rust\n        id: rust\n"
            f"        run: |\n          {check.IMAGE_OUTPUT}\n"), 1))
        self.has("before the cache step of the job ci")

    def test_a_rust_step_that_may_fail_is_found(self):
        self.edit(".github/workflows/check.yml", "        id: rust\n", "        id: rust\n        continue-on-error: true\n")
        self.has("no step rust")

    def test_the_key_of_an_actions_cache_step_is_checked(self):
        self.edit(".github/workflows/check.yml", "uses: actions/cache/restore@v4", "uses: actions/cache@v4")
        self.edit(".github/workflows/check.yml", "key: ${{ runner.os }}-${{ steps.rust.outputs.image }}-", "key: ${{ runner.os }}-")
        self.has("holds no image")

    def test_a_workflow_with_no_restore_is_found(self):
        self.edit(".github/workflows/release.yml", "uses: actions/cache/restore@v4", "uses: actions/checkout@v4")
        self.has("no step restores the cache")

    def test_a_missing_release_workflow_is_found(self):
        (self.root / ".github/workflows/release.yml").unlink()
        self.has("release.yml is missing")

    def test_a_stamp_command_inside_another_command_is_found(self):
        self.edit(".github/workflows/release.yml", f"        run: {check.STAMP}", f"        run: echo {check.STAMP}")
        self.has("has no step of only a name and run:")

    def test_a_key_from_another_output_is_found(self):
        self.edit(".github/workflows/release.yml", "key: ${{ steps.rust-cache.outputs.cache-primary-key }}",
                  "key: ${{ steps.rust-cache.outputs.cache-primary-key-other }}")
        self.has("holds no image")

    def test_an_image_only_in_a_comment_of_a_key_is_found(self):
        self.edit(".github/workflows/check.yml", "key: ${{ runner.os }}-${{ steps.rust.outputs.image }}-",
                  "key: ${{ runner.os }}-")
        self.edit(".github/workflows/check.yml", "-${{ github.sha }}\n", "-${{ github.sha }} # ${{ steps.rust.outputs.image }}\n")
        self.has("has a comment")

    def test_a_flow_mapping_is_found(self):
        path = self.root / ".github/workflows/check.yml"
        text = path.read_text()
        start = text.index("      - name: Restore the Rust cache")
        end = text.index("      - name: Verify complete vendored provider")
        path.write_text(text[:start] + "      - name: Restore the Rust cache\n        uses: actions/cache@v4\n"
                        "        with: { path: target/, key: linux-rust }\n" + text[end:])
        self.has("needs a line with: alone")

    def test_a_quoted_cache_action_is_found(self):
        self.edit(".github/workflows/check.yml", "uses: actions/cache/restore@v4", 'uses: "actions/cache/restore@v4"')
        self.has("names actions/cache in a form")

    def test_a_default_folder_in_the_release_workflow_is_found(self):
        self.edit(".github/workflows/release.yml", "jobs:\n", "defaults:\n  run:\n    working-directory: docs\njobs:\n")
        self.has("could move the step")

    def test_a_second_restore_after_the_stamp_step_is_found(self):
        self.edit(".github/workflows/release.yml", "      - name: Set up the project\n",
                  "      - name: Restore again\n        uses: actions/cache/restore@v4\n        with:\n"
                  "          path: target/\n          key: ${{ steps.rust.outputs.image }}-x\n"
                  "      - name: Set up the project\n")
        self.has("has no step of only a name and run:")

    def test_a_restore_key_that_looks_like_a_key_is_found(self):
        self.edit(".github/workflows/check.yml", "          restore-keys: |\n", "          restore-keys: |\n            rust:${{ runner.os }}-\n")
        self.has("the restore key rust:")

    def test_a_yaml_workflow_is_read(self):
        (self.root / ".github" / "workflows" / "extra.yaml").write_text(
            "jobs:\n  x:\n    steps:\n      - run: curl -fsSL https://raw.githubusercontent.com/x/y/main/install.sh | sh\n")
        self.has("extra.yaml: a script is fetched at https://raw.githubusercontent.com/x/y/main/")
    def test_a_test_in_a_nested_block_comment_is_found(self):
        (self.root / "tests" / "conformance.rs").write_text(
            "/* outer\n/* inner */\n#[test]\nfn plan_cli_and_evidence() {}\n*/\n#[test]\nfn renamed() {}\n")
        self.has("test(plan_cli_and_evidence) names no test")

    def test_a_block_comment_start_in_a_line_comment_is_no_comment(self):
        (self.root / "tests" / "conformance.rs").write_text(
            "#[test]\n// see /*\nfn renamed() {}\n// */\nfn plan_cli_and_evidence() {}\n")
        self.has("test(plan_cli_and_evidence) names no test")

    def test_a_quoted_or_qualified_just_before_the_stamp_is_found(self):
        for run in ('"just setup"', "'just setup'", "/usr/bin/just setup"):
            with self.subTest(run=run):
                self.setUp()
                self.edit(".github/workflows/release.yml", f"        run: {check.STAMP}", f"        run: {run}")
                self.has("has no step of only a name and run:")

    def test_a_cache_action_in_another_case_is_found(self):
        self.edit(".github/workflows/check.yml", "uses: actions/cache/restore@v4", "uses: Actions/Cache/restore@v4")
        self.has("names actions/cache in a form")

    def test_a_job_line_with_a_comment_is_found(self):
        self.edit(".github/workflows/check.yml", "\n  ci:\n", "\n  ci: # the check\n")
        self.has("is not under a job line of a plain name")

    def test_jobs_at_another_indent_are_found(self):
        (self.root / ".github" / "workflows" / "extra.yml").write_text(
            "jobs:\n    a:\n        steps:\n            - name: Rust\n              id: rust\n"
            f"              run: |\n                {check.IMAGE_OUTPUT}\n")
        self.has("extra.yml: the step name: Rust is not under a job line")

    def test_a_raw_url_in_another_case_or_over_http_is_found(self):
        for url in ("https://Raw.GitHubUserContent.com/x/y/v0.6.4", f"http://raw.githubusercontent.com/x/y/{'0' * 40}"):
            with self.subTest(url=url):
                self.setUp()
                (self.root / ".github" / "workflows" / "extra.yml").write_text(
                    f"jobs:\n  x:\n    steps:\n      - run: curl -fsSL {url}/install.sh | sh\n")
                self.has("extra.yml: a script is fetched at")

    def test_a_primary_key_of_a_step_that_does_not_restore_is_found(self):
        self.edit(".github/workflows/release.yml", "        id: rust-cache\n", "        id: restore\n")
        self.edit(".github/workflows/release.yml", "      - name: Set up the project\n",
                  "      - name: Fake key\n        id: rust-cache\n        run: echo cache-primary-key=x\n"
                  "      - name: Set up the project\n")
        self.has("is not of an earlier restore step")

    def test_a_test_file_that_cargo_renames_is_no_binary(self):
        self.edit(".config/nextest.toml", "binary(conformance)", "binary(conformance) | binary(glob)")
        (self.root / "tests" / "glob.rs").write_text("")
        self.has("binary(glob) names no test")

    def test_a_renamed_test_target_is_a_binary(self):
        self.edit(".config/nextest.toml", "binary(conformance)", "binary(conformance) | binary(glob_port)")
        (self.root / "tests" / "glob.rs").write_text("")
        self.assertEqual(check.nextest_problems(self.root), [])

    def test_no_stem_is_a_binary_without_autotests(self):
        path = self.root / "Cargo.toml"
        path.write_text(path.read_text().replace("[package]\n", "[package]\nautotests = false\n", 1))
        self.has("binary(conformance) names no test")

    def test_a_script_at_a_short_hash_is_found(self):
        self.edit(".github/workflows/release.yml", "rahuldave/jagt/604e3fb7754a168a28beeaa908908abd8589eee1/",
                  "rahuldave/jagt/604e3fb/")
        self.has("rahuldave/jagt/604e3fb/")

    def test_a_raw_url_whose_ref_cannot_be_read_is_found(self):
        for run in ("curl -fsSL https://raw.githubusercontent.com/rahuldave/jlog/${{ env.JLOG_REF }}/scripts/install.sh",
                    'BASE=https://raw.githubusercontent.com/rahuldave/jlog/main; curl -fsSL "$BASE/scripts/install.sh"',
                    'BASE=https://raw.githubusercontent.com/rahuldave/jlog; '
                    'curl -fsSL "$BASE/' + 'a' * 40 + '/scripts/install.sh"'):
            with self.subTest(run=run):
                self.setUp()
                (self.root / ".github" / "workflows" / "extra.yml").write_text(
                    f"jobs:\n  x:\n    steps:\n      - run: {run}\n")
                self.has("extra.yml: a script is fetched at")

    def test_an_image_output_inside_a_condition_is_found(self):
        self.edit(".github/workflows/check.yml", f"          {check.IMAGE_OUTPUT}\n",
                  f'          [[ -n "${{ImageOS:-}}" ]] && {check.IMAGE_OUTPUT}\n')
        self.has("no step rust")
    def test_threads_required_with_another_value_is_found(self):
        for value in ("'num-cpus'", "1"):
            with self.subTest(value=value):
                self.setUp()
                self.edit(".config/nextest.toml", "threads-required = 'num-test-threads'", f"threads-required = {value}")
                self.has('needs threads-required = "num-test-threads"')

    def test_defaults_of_a_job_in_the_release_workflow_are_found(self):
        self.edit(".github/workflows/release.yml", "  check:\n    runs-on: ubuntu-24.04\n",
                  "  check:\n    runs-on: ubuntu-24.04\n    defaults:\n      run:\n        working-directory: docs\n")
        self.has("could move the step")

    def test_a_save_step_before_its_restore_is_found(self):
        path = self.root / ".github/workflows/release.yml"
        text = path.read_text()
        start = text.index("      - name: Save the Rust cache")
        end = text.index("\n  release:")
        save = text[start:end].rstrip("\n") + "\n"
        text = text[:start] + text[end:]
        restore = text.index("      - name: Restore the Rust cache")
        path.write_text(text[:restore] + save + text[restore:])
        self.has("is not of an earlier restore step")

    def test_a_save_step_in_another_job_is_found(self):
        path = self.root / ".github/workflows/release.yml"
        text = path.read_text()
        start = text.index("      - name: Save the Rust cache")
        end = text.index("\n  release:")
        save = text[start:end].rstrip("\n") + "\n"
        text = text[:start] + text[end:]
        job = ("  save:\n    runs-on: ubuntu-24.04\n    steps:\n      - name: Rust\n        id: rust\n"
               f"        run: |\n          {check.IMAGE_OUTPUT}\n")
        path.write_text(text.replace("\n  release:", "\n" + job + save + "\n  release:", 1))
        self.has("is not of an earlier restore step")
    def test_a_step_in_another_list_form_is_found(self):
        for step in ("      -  run: just setup\n", "      - { name: Early setup, run: just setup }\n",
                     "      -\n        run: just setup\n"):
            with self.subTest(step=step):
                self.setUp()
                self.edit(".github/workflows/release.yml", "      - name: Drop the coverage stamp", step + "      - name: Drop the coverage stamp")
                self.has("is a list item in a form")

    def test_an_image_output_inside_an_if_block_is_found(self):
        self.edit(".github/workflows/check.yml", f"          {check.IMAGE_GUARD}\n          {check.IMAGE_OUTPUT}\n",
                  f'          if [ -n "${{ImageOS:-}}" ]; then\n          {check.IMAGE_OUTPUT}\n          fi\n')
        self.has("no step rust")

    def test_defaults_with_no_folder_in_the_release_workflow_are_found(self):
        for old, new in (("jobs:\n", "defaults:\n  run:\n    shell: sh\njobs:\n"),
                         ("  check:\n    runs-on: ubuntu-24.04\n",
                          "  check:\n    runs-on: ubuntu-24.04\n    defaults:\n      run:\n        shell: sh\n")):
            with self.subTest(new=new):
                self.setUp()
                self.edit(".github/workflows/release.yml", old, new)
                self.has("could move the step")


if __name__ == "__main__":
    unittest.main()
