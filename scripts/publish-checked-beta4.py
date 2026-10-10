"""Publish only the two successful, pinned beta.4 native builds."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import zipfile

REPO = 'mailingfeng/video-editor'
SHA = '23c18f01057f82c0f70a978c8cef97a9aeb925f4'
VERSION = '0.0.1-beta.4'
TAG = 'v' + VERSION
RUNS = {'windows': 38056052330, 'macos': 38056086207}
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
    return hashlib.file_digest(Path(path).open('rb'), 'sha256').hexdigest()


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
for path in ['package.json', 'package-lock.json', 'src-tauri/tauri.conf.json']:
    assert json.loads(Path(path).read_text())['version'] == VERSION
assert json.loads(Path('package-lock.json').read_text())['packages']['']['version'] == VERSION
assert f'version = "{VERSION}"' in Path('src-tauri/Cargo.toml').read_text()
assert f'name = "video-editor"\nversion = "{VERSION}"' in Path('src-tauri/Cargo.lock').read_text()

summary = {'version': VERSION, 'sourceCommit': SHA, 'tag': TAG, 'prerelease': True,
           'releaseStatus': 'NOT VERIFIED', 'platforms': {},
           'license': json.loads(Path('docs/evidence/license-validation-20261010.json').read_text()),
           'publisherRunUrl': f"https://github.com/{REPO}/actions/runs/{os.environ['GITHUB_RUN_ID']}",
           'limitations': ['Unsigned internal beta; Authenticode, Developer ID and notarization not complete.',
                           'Full manual installed GUI acceptance for this version remains pending.',
                           'Third-party distribution materials remain incomplete; see notices.',
                           'Latest content-variation batch pass was user reported; exact files and review category were not individually confirmed.']}

for platform, run_id in RUNS.items():
    run = api(f'/actions/runs/{run_id}')
    assert run['head_sha'] == SHA and run['head_branch'] == 'main'
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
    native_rust_count = rust_passes(unique(logs, 'backend-tests.txt'))
    assert native_rust_count >= 10
    assert len(re.findall(r'test license::tests::[^\r\n]+ \.\.\. ok', clean_log(unique(logs, 'backend-tests.txt')))) == 10
    assert re.search(r'Tests\s+37 passed', clean_log(unique(logs, 'frontend-tests.txt')))
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
                                    'tests': {'rust': native_rust_count, 'frontend': 37, 'generatedMedia': 14, 'installedMedia': 14},
                                    'artifacts': archives, 'steps': jobs[0]['steps']}
    print('Checked native build:', platform, published_name, flush=True)

copy_asset('docs/third-party-notices.md', 'third-party-notices.md')
copy_asset('docs/license.md', 'license.md')
(ASSETS / 'verification-summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
files = sorted(ASSETS.iterdir())
assert len(files) == 13 and all(p.is_file() for p in files)
assert not any(p.suffix.lower() in ['.mp4', '.mov', '.mkv', '.webm'] for p in files)
(ASSETS / 'SHA256SUMS').write_text(''.join(f'{sha(p)}  {p.name}\n' for p in files))
expected = {p.name: {'sha256': sha(p), 'size': p.stat().st_size} for p in ASSETS.iterdir()}
assert len(expected) == 14
assert api('/git/ref/heads/main')['object']['sha'] == SHA
api('/git/refs', 'POST', {'ref': 'refs/tags/' + TAG, 'sha': SHA})
notes = Path('scripts/beta4-release-notes.md').read_text()
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
