/* The demo player.
 *
 * Every transcript below is the real output of running that command against
 * helm/helm at a8ab76e. Nothing here is executing the tool — a GitHub Pages site is
 * static, and pretending otherwise would be the one thing this project is against.
 * Paths are shown relative to the repository root; everything else is verbatim.
 */

var STEPS = [
  {
    label: 'What is in here?',
    title: 'helm/helm @ a8ab76e',
    caption:
      'Nothing is indexed until you ask. ERRORS counts files that did not parse — those thirteen ' +
      'are chart fixtures helm keeps deliberately broken to test its own error messages. They ' +
      'matter because a file that does not parse is invisible to every analysis after this one.',
    lines: [
      ['cmd', 'fr parse --stats'],
      ['', 'LANGUAGE       FILES   ERRORS   UNREAD'],
      ['', 'bash              20        0        0'],
      ['', 'go               539        0        0'],
      ['', 'helm             669       13        0'],
      ['', 'markdown          95        0        0'],
      ['', 'yaml              83        0        0']
    ]
  },
  {
    label: 'Naming a target',
    title: 'ambiguity is not resolved for you',
    caption:
      'helm carries a v2 and a v3 of several packages side by side. Rather than picking one, the ' +
      'tool shows both and asks for a position. A coin flip that reads like an answer is worse ' +
      'than a refusal.',
    lines: [
      ['cmd', 'fr refs RunAll'],
      ['refuse', "Error: 'RunAll' is defined 2 times; specify a position as path:line:col"],
      ['', '  RunAll (function) in internal/chart/v3/lint/lint.go'],
      ['', '  RunAll (function) in pkg/chart/v2/lint/lint.go']
    ]
  },
  {
    label: 'Where is it used?',
    title: 'confidence per site',
    caption:
      'Five uses across four files, every one proved. The tag is the rule that resolved it: ' +
      'exact means the scope chain, the file or the Go package says so. Only exact and ' +
      'import-qualified are ever rewritten.',
    lines: [
      ['cmd', 'fr refs pkg/action/action.go:725:6'],
      ['', '5 reference(s) to determineReleaseSSApplyMethod'],
      ['exact', '  pkg/action/action_test.go:2280:54  [exact]'],
      ['exact', '  pkg/action/action_test.go:2281:54  [exact]'],
      ['exact', '  pkg/action/install.go:676:23  [exact]'],
      ['exact', '  pkg/action/rollback.go:192:23  [exact]'],
      ['exact', '  pkg/action/upgrade.go:334:23  [exact]']
    ]
  },
  {
    label: 'Who calls it?',
    title: 'the call graph, upward',
    caption:
      'The same edges read in the other direction. Two levels up from a private helper is the ' +
      'public API — which is also why dead-code analysis has to treat exported symbols as roots ' +
      'in a library.',
    lines: [
      ['cmd', 'fr callers pkg/action/action.go:725:6 --depth 2'],
      ['', 'determineReleaseSSApplyMethod'],
      ['', '  TestDetermineReleaseSSAApplyMethod'],
      ['', '  Install::createRelease'],
      ['', '  Rollback::prepareRollback'],
      ['', '  Upgrade::prepareUpgrade'],
      ['dim', '    Install::RunWithContext'],
      ['dim', '    Rollback::Run'],
      ['dim', '    Upgrade::RunWithContext']
    ]
  },
  {
    label: 'What is dead?',
    title: 'fr unused',
    caption:
      '--language and --path narrow the report, never the index: scanning a subdirectory instead ' +
      'would hide the callers and invent dead code. --internal drops exported symbols, because a ' +
      'library’s public API has no caller in its own repository. 47 findings, none of them ' +
      'functions or methods — helm has very little dead code, and it took eight bug fixes to be ' +
      'able to say that.',
    lines: [
      ['cmd', 'fr unused --language go --internal'],
      ['', 'parameter    lo                     internal/chart/v3/lint/lint.go'],
      ['', 'parameter    targetDir              internal/plugin/installer/extractor.go'],
      ['', 'parameter    archiveData            internal/plugin/installer/http_installer.go'],
      ['', 'parameter    provData               internal/plugin/installer/http_installer.go'],
      ['dim', '…'],
      ['', ''],
      ['', '47 symbol(s) with no detected use, of 427 found across the workspace']
    ]
  },
  {
    label: 'What is written twice?',
    title: 'fr duplicates',
    caption:
      'Structure is compared, not text, so a copy whose variables were renamed still matches — ' +
      'the copy grep will never find. Those five blocks in show.go are five cobra subcommands ' +
      'built the same way in a row. Across all of helm: 337 blocks, 64,530 redundant tokens, 3.6 ' +
      'seconds.',
    lines: [
      ['cmd', 'fr duplicates --language go --path pkg/cmd --path pkg/action --min-tokens 100'],
      ['', '5 copies, 107 tokens each (428 redundant) — go'],
      ['', '  pkg/cmd/show.go:79-98'],
      ['', '  pkg/cmd/show.go:100-119'],
      ['', '  pkg/cmd/show.go:121-140'],
      ['', '  pkg/cmd/show.go:142-161'],
      ['', '  pkg/cmd/show.go:163-182'],
      ['', '3 copies, 180 tokens each (360 redundant) — go'],
      ['', '  pkg/cmd/get_hooks_test.go:1-51'],
      ['', '  pkg/cmd/get_manifest_test.go:1-51'],
      ['', '  pkg/cmd/get_notes_test.go:1-51']
    ]
  },
  {
    label: 'The change',
    title: 'diff first — nothing is written',
    caption:
      'determineReleaseSSApplyMethod is a poor name: SS is "server-side", which nothing in the ' +
      'identifier says. Every mutating command prints a diff and changes nothing until --write.',
    lines: [
      ['cmd', 'fr rename pkg/action/action.go:725:6 releaseApplyMethod'],
      ['dim', '--- a/pkg/action/action.go'],
      ['dim', '+++ b/pkg/action/action.go'],
      ['del', '-func determineReleaseSSApplyMethod(serverSideApply bool) release.ApplyMethod {'],
      ['add', '+func releaseApplyMethod(serverSideApply bool) release.ApplyMethod {'],
      ['dim', '--- a/pkg/action/install.go'],
      ['del', '-\t\tApplyMethod: string(determineReleaseSSApplyMethod(i.ServerSideApply)),'],
      ['add', '+\t\tApplyMethod: string(releaseApplyMethod(i.ServerSideApply)),'],
      ['', ''],
      ['', 'determineReleaseSSApplyMethod → releaseApplyMethod: 5 site(s) across 4 file(s)']
    ]
  },
  {
    label: 'What it would not do',
    title: 'the list under the diff',
    caption:
      'The important half. Thirteen chart fixtures do not parse, so a reference inside one is a ' +
      'reference the tool cannot promise it found — it says so rather than assuming. It also left ' +
      'TestDetermineReleaseSSAApplyMethod alone: that identifier contains the old name and is not ' +
      'it, which is the case find-and-replace gets wrong.',
    lines: [
      ['weak', 'Not changed — review these yourself:'],
      ['weak', '  parse-errors (13):'],
      ['', '    internal/chart/v3/lint/rules/testdata/malformed-template/templates/bad.yaml:1:1'],
      ['dim', '      file has syntax errors; references in it may be missing'],
      ['dim', '    … and 12 more'],
      ['', ''],
      ['dim', 'Nothing written. Re-run with --write to apply.']
    ]
  },
  {
    label: 'Apply, and prove it',
    title: 'the compiler is the verification',
    caption:
      'The reparse check inside the tool only proves the file still parses. Whether the program ' +
      'still means what it meant is the language toolchain’s question, and it is the one that ' +
      'counts.',
    lines: [
      ['cmd', 'fr rename pkg/action/action.go:725:6 releaseApplyMethod --write'],
      ['', 'Applied to 5 file(s).'],
      ['', ''],
      ['cmd', 'go build ./...'],
      ['cmd', "go test ./pkg/action/ -run 'TestDetermineReleaseSSAApplyMethod|TestIsDryRun'"],
      ['exact', 'ok  \thelm.sh/helm/v4/pkg/action\t0.585s']
    ]
  },
  {
    label: 'Open the PR',
    title: 'ordinary git work',
    caption:
      'Six lines across five files, reviewable one by one, with nothing tool-specific in the ' +
      'diff. Two habits: commit the refactor on its own, and paste the refusals into the ' +
      'description — a reviewer cannot know to check the unparseable files unless you say so.',
    lines: [
      ['cmd', 'git checkout -b rename-release-apply-method'],
      ['cmd', 'git diff --stat'],
      ['', ' pkg/action/action.go      | 2 +-'],
      ['', ' pkg/action/action_test.go | 4 ++--'],
      ['', ' pkg/action/install.go     | 2 +-'],
      ['', ' pkg/action/rollback.go    | 2 +-'],
      ['', ' pkg/action/upgrade.go     | 2 +-'],
      ['', ' 5 files changed, 6 insertions(+), 6 deletions(-)'],
      ['', ''],
      ['cmd', 'git commit -am "Rename determineReleaseSSApplyMethod to releaseApplyMethod"'],
      ['cmd', 'gh pr create --fill'],
      ['exact', 'https://github.com/helm/helm/pull/…']
    ]
  }
];

(function () {
  var screen = document.getElementById('screen');
  var caption = document.getElementById('caption');
  var title = document.getElementById('term-title');
  var list = document.getElementById('steps');
  var progress = document.getElementById('progress');
  if (!screen || !list) return;

  var current = 0;
  var playing = null;

  STEPS.forEach(function (step, i) {
    var li = document.createElement('li');
    var button = document.createElement('button');
    button.innerHTML = '<span class="n">' + String(i + 1).padStart(2, '0') + '</span>' + step.label;
    button.addEventListener('click', function () { stop(); show(i); });
    li.appendChild(button);
    list.appendChild(li);
  });

  function render(step) {
    screen.textContent = '';
    step.lines.forEach(function (line) {
      var kind = line[0];
      var text = line[1];
      var el = document.createElement('span');
      if (kind) el.className = kind;
      el.textContent = text;
      screen.appendChild(el);
      screen.appendChild(document.createTextNode('\n'));
    });
  }

  function show(i) {
    current = ((i % STEPS.length) + STEPS.length) % STEPS.length;
    var step = STEPS[current];
    render(step);
    caption.textContent = step.caption;
    title.textContent = step.title;
    progress.textContent = current + 1 + ' / ' + STEPS.length;
    Array.prototype.forEach.call(list.querySelectorAll('button'), function (b, n) {
      b.setAttribute('aria-selected', n === current ? 'true' : 'false');
    });
  }

  function stop() {
    if (playing) { clearInterval(playing); playing = null; }
    document.getElementById('play').textContent = 'Play all';
  }

  document.getElementById('next').addEventListener('click', function () { stop(); show(current + 1); });
  document.getElementById('prev').addEventListener('click', function () { stop(); show(current - 1); });
  document.getElementById('play').addEventListener('click', function () {
    if (playing) { stop(); return; }
    this.textContent = 'Pause';
    playing = setInterval(function () {
      if (current === STEPS.length - 1) { stop(); return; }
      show(current + 1);
    }, 5200);
  });

  document.addEventListener('keydown', function (e) {
    if (e.key === 'ArrowRight') { stop(); show(current + 1); }
    if (e.key === 'ArrowLeft') { stop(); show(current - 1); }
  });

  show(0);
})();
