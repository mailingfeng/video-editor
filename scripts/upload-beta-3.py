import hashlib
import io
import json
import os
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath

REPO = 'mailingfeng/video-editor'
assert os.environ['GITHUB_REPOSITORY'] == REPO
config = json.loads(Path('publication.json').read_text())
SOURCE = config['sourceCommit']
VERSION = config['version']
RELEASE = config['releaseId']
HEADERS = {'Authorization': 'Bearer '+os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28'}

def api(path, method='GET'):
    request = urllib.request.Request('https://api.github.com/repos/'+REPO+path, headers=HEADERS, method=method)
    with urllib.request.urlopen(request, timeout=120) as response:
        data = response.read()
    return json.loads(data) if data else None

def sha(data):
    return hashlib.sha256(data).hexdigest()

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, new_url):
        return None

def archive_bytes(artifact):
    assert not artifact['expired'] and artifact['workflow_run']['head_sha'] == SOURCE
    assert artifact['digest'].startswith('sha256:')
    request = urllib.request.Request('https://api.github.com/repos/'+REPO+'/actions/artifacts/'+str(artifact['id'])+'/zip', headers=HEADERS)
    try:
        urllib.request.build_opener(NoRedirect).open(request, timeout=120)
        raise AssertionError('Expected signed artifact redirect')
    except urllib.error.HTTPError as error:
        assert error.code == 302
        location = error.headers['Location']
    assert location.startswith('https://')
    # The GitHub token never goes to the signed storage URL.
    with urllib.request.urlopen(location, timeout=180) as response:
        data = response.read()
    assert len(data) == artifact['size_in_bytes']
    assert 'sha256:'+sha(data) == artifact['digest']
    return data

def unique(archive, basename):
    matches = [name for name in archive.namelist() if PurePosixPath(name).name == basename]
    assert len(matches) == 1, (basename, matches)
    return archive.read(matches[0])

release = api('/releases/'+str(RELEASE))
assert release['draft'] and release['prerelease'] and release['tag_name'] == config['tag']
assert release['target_commitish'] == SOURCE and SOURCE in release['body']
tag = api('/git/ref/tags/'+config['tag'])['object']
if tag['type'] == 'tag':
    tag = api('/git/tags/'+tag['sha'])['object']
assert tag['type'] == 'commit' and tag['sha'] == SOURCE
assert set(config['workflows']) == {'windows_x64', 'mac_intel'}
assets = {}
summary = {'version': VERSION, 'sourceCommit': SOURCE, 'verifiedAt': datetime.now(timezone.utc).isoformat(), 'channel': 'prerelease', 'releaseStatus': 'NOT VERIFIED', 'platforms': {}, 'uploaderRunUrl': 'https://github.com/'+REPO+'/actions/runs/'+os.environ['GITHUB_RUN_ID']}

for platform, entry in config['workflows'].items():
    run = api('/actions/runs/'+str(entry['runId']))
    assert run['head_sha'] == SOURCE and run['status'] == 'completed' and run['conclusion'] == 'success'
    jobs = api('/actions/runs/'+str(entry['runId'])+'/jobs')['jobs']
    assert jobs and all(job['conclusion'] == 'success' for job in jobs)
    for required in ['Frontend tests', 'Backend tests', 'Generated media regression', 'Desktop clippy', 'Media regression using installed tools']:
        matches = [step for job in jobs for step in job['steps'] if step['name'] == required]
        assert len(matches) == 1 and matches[0]['conclusion'] == 'success', (platform, required)
    artifacts = api('/actions/runs/'+str(entry['runId'])+'/artifacts')['artifacts']
    label = 'windows-x64' if platform == 'windows_x64' else 'macos-intel'
    install = [a for a in artifacts if a['name'].startswith('frameshift-'+VERSION+'-'+label+'-development-')]
    logs = [a for a in artifacts if a['name'].startswith(label+'-verification-')]
    assert len(install) == 1 and len(logs) == 1
    data = archive_bytes(install[0])
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        assert archive.testzip() is None
        build_data = unique(archive, 'build.json')
        build = json.loads(build_data.decode('utf-8-sig'))
        resources_data = unique(archive, 'installed-resources.json')
        resources = json.loads(resources_data.decode('utf-8-sig'))
        target = 'x86_64-pc-windows-msvc' if platform == 'windows_x64' else 'x86_64-apple-darwin'
        assert build['sourceCommit'] == SOURCE and build['target'] == target
        assert resources['target'] == target and resources['host'] == target
        assert resources['exitCode'] == 0 and resources['status'] == 'DEVELOPMENT CHECKED' and not resources['errors']
        assert set(resources['tools']) == {'ffmpeg', 'ffprobe'}
        assert VERSION in build['installer']
        installer = unique(archive, build['installer'])
        assert len(installer) == build['installerSizeBytes'] and sha(installer) == build['installerSha256']
        name = 'frameshift_'+VERSION+('_windows_x64_setup.exe' if platform == 'windows_x64' else '_macos_intel.dmg')
        prefix = 'windows' if platform == 'windows_x64' else 'macos'
        guide = 'windows-desktop-acceptance.md' if platform == 'windows_x64' else 'macos-intel-desktop-acceptance.md'
        assets[name] = installer
        assets[prefix+'-build.json'] = build_data
        assets[prefix+'-installed-resources.json'] = resources_data
        assets[guide] = unique(archive, guide)
    log_data = archive_bytes(logs[0])
    with zipfile.ZipFile(io.BytesIO(log_data)) as archive:
        assert archive.testzip() is None
    assets[prefix+'-verification-logs.zip'] = log_data
    summary['platforms'][platform] = {'target': target, 'sourceCommit': SOURCE, 'runId': entry['runId'], 'runUrl': entry['url'], 'conclusion': 'success', 'artifact': name, 'sizeBytes': len(installer), 'sha256': sha(installer), 'resourcesStatus': resources['status'], 'releaseStatus': resources['releaseStatus'], 'jobs': [{'name': job['name'], 'conclusion': job['conclusion'], 'steps': [{'name': s['name'], 'conclusion': s['conclusion']} for s in job['steps']]} for job in jobs]}
    print(platform+': installer source, native checks and artifact hashes verified', flush=True)

assets['third-party-notices.md'] = Path('docs/third-party-notices.md').read_bytes()
assets['verification-summary.json'] = (json.dumps(summary, ensure_ascii=False, indent=2)+'\n').encode()
assets['SHA256SUMS'] = ''.join(sha(data)+'  '+name+'\n' for name, data in sorted(assets.items())).encode()
assert len(assets) == 13
for name, data in sorted(assets.items()):
    current = api('/releases/'+str(RELEASE))
    assert current['draft'] and current['tag_name'] == config['tag']
    existing = [a for a in current['assets'] if a['name'] == name]
    assert len(existing) <= 1
    if existing and existing[0]['state'] == 'starter' and existing[0].get('digest') is None:
        api('/releases/assets/'+str(existing[0]['id']), 'DELETE')
        existing = []
    if existing:
        uploaded = existing[0]
    else:
        request = urllib.request.Request('https://uploads.github.com/repos/'+REPO+'/releases/'+str(RELEASE)+'/assets?'+urllib.parse.urlencode({'name': name}), data=data, headers={**HEADERS, 'Content-Type': 'application/octet-stream'}, method='POST')
        with urllib.request.urlopen(request, timeout=300) as response:
            uploaded = json.load(response)
    assert uploaded['name'] == name and uploaded['state'] == 'uploaded'
    assert uploaded['size'] == len(data) and uploaded['digest'] == 'sha256:'+sha(data)
    print(json.dumps({'asset': name, 'sizeBytes': len(data), 'sha256': sha(data), 'verified': True}), flush=True)
final = api('/releases/'+str(RELEASE))
assert final['draft'] and {a['name'] for a in final['assets']} == set(assets)
print('Both platforms and all 13 draft release assets uploaded and verified; public publication remains with the caller.', flush=True)
