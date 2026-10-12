"""Publish only the two successful, pinned beta.6 native builds."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import zipfile

REPO = 'mailingfeng/video-editor'
SHA = '1c712fdd2d39154c1d7da821df1220f3f684c1ff'
VERSION = '0.0.1-beta.6'
TAG = 'v' + VERSION
RUNS = {'windows': 38252441858, 'macos': 38252443084}
TARGETS = {'windows': 'x86_64-pc-windows-msvc', 'macos': 'x86_64-apple-darwin'}
ROOT = Path('.release-work')
ASSETS = ROOT / 'assets'
ASSETS.mkdir(parents=True, exist_ok=True)


def gh(*args, output=None):
    if output:
        with Path(output).open('wb') as handle:
            subprocess.run(['gh', *args], stdout=handle, check=True)
        return None
    return subprocess.check_output(['gh', *args], text=True)


def api(path, method='GET', body=None):
    args = ['api', f'repos/{REPO}{path}', '--method', method]
    if body is not None:
        body_path = ROOT / 'api-body.json'
        body_path.write_text(json.dumps(body))
        args.extend(['--input', str(body_path)])
    result = gh(*args)
    return json.loads(result) if result.strip() else None


def sha(path):
    with Path(path).open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def clean_log(path):
    return re.sub(r'\x1b\[[0-9;]*m', '', path.read_text(encoding='utf-8-sig'))


def rust_passes(path):
    return sum(map(int, re.findall(r'test result: ok\. (\d+) passed', clean_log(path))))


def unique(folder, name):
    matches = list(folder.rglob(name))
    assert len(matches) == 1, (folder, name, matches)
    return matches[0]


def copy_asset(source, name):
    destination = ASSETS / name
    destination.write_bytes(Path(source).read_bytes())
    return destination


assert api('/git/ref/heads/main')['object']['sha'] == SHA
assert subprocess.check_output(['git', 'merge-base', 'HEAD', SHA], text=True).strip() == SHA
publisher_files = subprocess.check_output(['git', 'diff', '--name-only', SHA, 'HEAD'], text=True).splitlines()
assert set(publisher_files) == {'.github/workflows/macos-intel.yml',
                                'scripts/publish-checked-beta6.py'}
for path in ['package.json', 'package-lock.json', 'src-tauri/tauri.conf.json']:
    assert json.loads(Path(path).read_text())['version'] == VERSION
assert json.loads(Path('package-lock.json').read_text())['packages']['']['version'] == VERSION
assert f'version = "{VERSION}"' in Path('src-tauri/Cargo.toml').read_text()
assert f'name = "video-editor"\nversion = "{VERSION}"' in Path('src-tauri/Cargo.lock').read_text()

summary = {'version': VERSION, 'sourceCommit': SHA, 'tag': TAG, 'prerelease': True,
           'releaseStatus': 'NOT VERIFIED', 'platforms': {},
           'localValidation': json.loads(Path('docs/evidence/beta6-validation-20261012.json').read_text()),
           'publisherRunUrl': f"https://github.com/{REPO}/actions/runs/{os.environ['GITHUB_RUN_ID']}",
           'limitations': ['Unsigned internal beta; Authenticode, Developer ID and notarization not complete.',
                           'Full manual installed GUI acceptance for this version remains pending.',
                           'Third-party distribution materials remain incomplete; see notices.']}
tool_lock = json.loads(Path('tools/sidecars.lock.json').read_text())
assert summary['localValidation']['version'] == VERSION

for platform, run_id in RUNS.items():
    run = api(f'/actions/runs/{run_id}')
    assert run['head_sha'] == SHA and run['head_branch'] == 'main'
    assert run['path'] == ('.github/workflows/windows-x64.yml' if platform == 'windows' else '.github/workflows/macos-intel.yml')
    assert run['status'] == 'completed' and run['conclusion'] == 'success'
    jobs = api(f'/actions/runs/{run_id}/jobs')['jobs']
    assert len(jobs) == 1 and jobs[0]['conclusion'] == 'success'
    assert all(step['conclusion'] == 'success' for step in jobs[0]['steps'])
    metadata = api(f'/actions/runs/{run_id}/artifacts')['artifacts']
    assert len(metadata) == 2
    folders = {}
    archives = {}
    for artifact in metadata:
        assert not artifact['expired'] and artifact['workflow_run']['head_sha'] == SHA
        kind = 'development' if '-development-' in artifact['name'] else 'logs'
        if kind == 'development':
            assert VERSION in artifact['name']
        assert kind not in folders
        archive = ROOT / f"{artifact['id']}.zip"
        gh('api', f"repos/{REPO}/actions/artifacts/{artifact['id']}/zip", output=archive)
        assert artifact['digest'] == 'sha256:' + sha(archive)
        folder = ROOT / artifact['name']
        folder.mkdir()
        with zipfile.ZipFile(archive) as zipped:
            for member in zipped.infolist():
                assert (folder / member.filename).resolve().is_relative_to(folder.resolve())
            zipped.extractall(folder)
        folders[kind] = folder
        archives[kind] = {'id': artifact['id'], 'name': artifact['name'], 'digest': artifact['digest']}
        if kind == 'logs':
            copy_asset(archive, platform + '-verification-logs.zip')
    development = folders['development']
    logs = folders['logs']
    assert not any(p.suffix.lower() in ['.mp4', '.mov', '.mkv', '.webm'] for p in logs.rglob('*'))
    prepared = json.loads(unique(logs, 'sidecars.json').read_text(encoding='utf-8-sig'))
    assert prepared['status'] == 'RESOURCES PREPARED' and prepared['target'] == TARGETS[platform]
    assert set(prepared['tools']) == {'ffmpeg', 'ffprobe'}
    for name, tool in prepared['tools'].items():
        assert tool['architecture'] == TARGETS[platform]
        assert tool['sha256'] == tool_lock['targets'][TARGETS[platform]]['tools'][name]['binarySha256']
    recovered_tools = platform == 'macos' and 'Recovered locked tools from the SHA-256-verified beta.4 installer after HTTP 429' in clean_log(unique(logs, 'sidecar-download.txt'))
    build = json.loads(unique(development, 'build.json').read_text(encoding='utf-8-sig'))
    resources = json.loads(unique(development, 'installed-resources.json').read_text(encoding='utf-8-sig'))
    assert build['sourceCommit'] == SHA and build['target'] == TARGETS[platform]
    assert build['runUrl'] == run['html_url']
    assert VERSION in build['installer']
    installer = unique(development, build['installer'])
    assert installer.stat().st_size == build['installerSizeBytes']
    assert sha(installer) == build['installerSha256']
    assert resources['target'] == TARGETS[platform]
    assert resources['status'] == 'DEVELOPMENT CHECKED' and resources['exitCode'] == 0
    assert resources['errors'] == [] and resources['releaseStatus'] == 'NOT VERIFIED'
    assert resources['host'] == TARGETS[platform]
    assert resources['main']['architecture'] == TARGETS[platform]
    assert resources['hardening'] == {'templateScan': 'passed', 'debugArtifacts': []}
    assert json.loads(unique(logs, 'build.json').read_text(encoding='utf-8-sig')) == build
    assert json.loads(unique(logs, 'installed-resources.json').read_text(encoding='utf-8-sig')) == resources
    for name, tool in resources['tools'].items():
        assert tool['architecture'] == TARGETS[platform]
        assert tool['sha256'] == tool_lock['targets'][TARGETS[platform]]['tools'][name]['binarySha256']
    upgrade = None
    if platform == 'windows':
        upgrade = json.loads(unique(development, 'installer-upgrade.json').read_text(encoding='utf-8-sig'))
        assert upgrade == json.loads(unique(logs, 'installer-upgrade.json').read_text(encoding='utf-8-sig'))
        assert upgrade['sourceCommit'] == SHA and upgrade['version'] == VERSION
        assert upgrade['baselineVersion'] == '0.0.1-beta.4'
        assert upgrade['baselineSha256'] == '861b770592c54c8e0f8be02dfa26a660035a105d77ac09ee8f6a39c27ce5b810'
        assert upgrade['result'] == 'passed'
        expected_cases = {
            'baselineMaintenance': {'result': 'maintenance-choice'},
            'upgradeFromBeta4': {'result': 'complete', 'exitCode': 0},
            'reinstallSameVersion': {'result': 'complete', 'exitCode': 0},
            'repairMissingLocationKey': {'result': 'complete', 'exitCode': 0},
            'repairMissingUninstallerAndVersion': {'result': 'complete', 'exitCode': 0},
            'guiDowngrade': {'result': 'downgrade-blocked', 'exitCode': 2},
            'silentDowngrade': {'exitCode': 2},
            'passiveDowngrade': {'result': 'downgrade-blocked', 'exitCode': 2},
            'runningApp': {'result': 'running-blocked', 'exitCode': 2},
            'silentRunningApp': {'exitCode': 2},
            'silentRepair': {'exitCode': 0},
            'passiveRepair': {'result': 'complete', 'exitCode': 0},
        }
        assert set(upgrade['cases']) == set(expected_cases)
        for name, wanted in expected_cases.items():
            assert all(upgrade['cases'][name][key] == value for key, value in wanted.items()), name
        assert upgrade['shortcut']['exists']
        assert upgrade['shortcut']['rawTarget'] == upgrade['shortcut']['expected']
        copy_asset(unique(development, 'installer-upgrade.json'), 'windows-installer-upgrade.json')
    native_rust_count = rust_passes(unique(logs, 'backend-tests.txt'))
    assert native_rust_count == 96
    assert len(re.findall(r'test license::tests::[^\r\n]+ \.\.\. ok', clean_log(unique(logs, 'backend-tests.txt')))) == 10
    assert re.search(r'Tests\s+35 passed', clean_log(unique(logs, 'frontend-tests.txt')))
    assert re.search(r'(?:#|ℹ)\s+pass 19\b', clean_log(unique(logs, 'node-tests.txt')))
    assert 'test jobs::log::tests::report_dates_follow_beijing_midnight_leap_day_and_new_year ... ok' in clean_log(unique(logs, 'backend-tests.txt'))
    assert len(re.findall(r'test (?:report_keeps_timed_stages_and_coarse_progress_without_tool_output|report_and_snapshot_keep_disk_full_advice_without_raw_failure_details|old_history_is_rebuilt_from_saved_times_and_result_without_replaying_diagnostics|time_only_history_gets_dates_from_the_saved_task_times) \.\.\. ok', clean_log(unique(logs, 'backend-tests.txt')))) == 4
    assert rust_passes(unique(logs, 'generated-media.txt')) == 14
    installed_count = rust_passes(unique(logs, 'installed-media.txt'))
    if platform == 'windows':
        installed_count += rust_passes(unique(logs, 'installed-lifecycle.txt'))
        installed_count += rust_passes(unique(logs, 'installed-batch.txt'))
    assert installed_count == 14
    published_name = f'frameshift_{VERSION}_windows_x64_setup.exe' if platform == 'windows' else f'frameshift_{VERSION}_macos_intel.dmg'
    copy_asset(installer, published_name)
    copy_asset(unique(development, 'build.json'), platform + '-build.json')
    copy_asset(unique(development, 'installed-resources.json'), platform + '-installed-resources.json')
    guide = 'windows-desktop-acceptance.md' if platform == 'windows' else 'macos-intel-desktop-acceptance.md'
    # Windows checkout uses CRLF. Compare Markdown text with universal
    # newlines, then publish the exact file from the checked native artifact.
    assert unique(development, guide).read_text(encoding='utf-8-sig') == Path('docs', guide).read_text(encoding='utf-8-sig')
    copy_asset(unique(development, guide), guide)
    summary['platforms'][platform] = {'target': TARGETS[platform], 'runId': run_id, 'runUrl': run['html_url'],
                                    'sourceCommit': SHA, 'installer': published_name,
                                    'installerSha256': build['installerSha256'], 'installerSizeBytes': build['installerSizeBytes'],
                                    'installedMainSha256': resources['main']['sha256'], 'installedFingerprint': resources['fingerprint'],
                                    'toolPreparation': prepared, 'lockedToolRecoveryUsed': recovered_tools,
                                    'tests': {'rust': native_rust_count, 'frontend': 35, 'releaseAndSidecar': 19, 'generatedMedia': 14, 'installedMedia': 14},
                                    'artifacts': archives, 'steps': jobs[0]['steps']}
    if upgrade:
        summary['platforms'][platform]['tests']['installerScenarios'] = len(upgrade['cases'])
    summary['platforms'][platform]['hardening'] = resources['hardening']
    print('Checked native build:', platform, published_name, flush=True)

copy_asset('docs/third-party-notices.md', 'third-party-notices.md')
copy_asset('docs/license.md', 'license.md')
(ASSETS / 'verification-summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
files = sorted(ASSETS.iterdir())
assert len(files) == 14 and all(p.is_file() for p in files)
assert not any(p.suffix.lower() in ['.mp4', '.mov', '.mkv', '.webm'] for p in files)
(ASSETS / 'SHA256SUMS').write_text(''.join(f'{sha(p)}  {p.name}\n' for p in files))
expected = {p.name: {'sha256': sha(p), 'size': p.stat().st_size} for p in ASSETS.iterdir()}
assert len(expected) == 15
assert api('/git/ref/heads/main')['object']['sha'] == SHA
api('/git/refs', 'POST', {'ref': 'refs/tags/' + TAG, 'sha': SHA})
notes = Path('docs/releases/0.0.1-beta.6.md').read_text()
notes += f'\n原生验证：每个平台通过 96 项后端、35 项界面、19 项发布工具、14 项生成媒体及 14 项安装后媒体回归；Windows 另通过 12 个安装与保护场景。\n\n[Windows 验证](https://github.com/{REPO}/actions/runs/{RUNS["windows"]}) · [Mac Intel 验证](https://github.com/{REPO}/actions/runs/{RUNS["macos"]})。\n\n源提交：`{SHA}`；标签：`{TAG}`。全部 15 个发布附件已核对大小与 SHA-256。\n'
release = api('/releases', 'POST', {'tag_name': TAG, 'target_commitish': SHA, 'name': '帧序 ' + VERSION + ' 内测',
                                  'body': notes, 'draft': True, 'prerelease': True})
gh('release', 'upload', TAG, *map(str, sorted(ASSETS.iterdir())), '--repo', REPO)


def verify_assets(release):
    actual = {item['name']: item for item in release['assets']}
    assert set(actual) == set(expected)
    for name, wanted in expected.items():
        assert actual[name]['state'] == 'uploaded'
        assert actual[name]['size'] == wanted['size']
        assert actual[name]['digest'] == 'sha256:' + wanted['sha256']


verify_assets(api(f"/releases/{release['id']}"))
assert api('/git/ref/tags/' + TAG)['object']['sha'] == SHA
api(f"/releases/{release['id']}", 'PATCH', {'draft': False, 'prerelease': True})
release = api('/releases/tags/' + TAG)
assert not release['draft'] and release['prerelease']
verify_assets(release)
receipt = {'version': VERSION, 'sourceCommit': SHA, 'releaseId': release['id'], 'releaseUrl': release['html_url'],
           'tag': TAG, 'prerelease': True, 'allUploadedDigestsVerified': True, 'assets': expected,
           'publisherRunUrl': summary['publisherRunUrl']}
(ROOT / 'publication.json').write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + '\n')
print('Published and verified:', release['html_url'], flush=True)
