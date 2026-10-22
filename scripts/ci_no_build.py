"""Audited no-build tooling environment, not a security sandbox.

PATH consumers inherit exact metadata/version-only compiler shims. Windows
Python direct compiler launches also use a checked startup hook; native children,
shell command strings and Python -S/-I or scrubbed startup environments remain
outside this reviewed mechanism and require source review.
Application discovery points at an empty directory, never a stale executable.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import uuid

ALLOWED = {
    'cargo': (('-V',), ('--version',),
              ('metadata', '--format-version', '1', '--no-deps', '--locked')),
    'rustc': (('-V',), ('-Vv',), ('--version',)),
}


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source_digest():
    # Python parses universal newlines; Git checkout newline conversion must
    # not make the same reviewed source unverifiable on a foreign-OS aggregate.
    return hashlib.sha256(Path(__file__).read_text(encoding='utf-8').encode()).hexdigest()


def strict_json(text):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('Duplicate no-build evidence key')
            result[key] = value
        return result
    def invalid(value):
        raise ValueError(f'Non-finite no-build evidence: {value}')
    return json.loads(text, object_pairs_hook=unique, parse_constant=invalid)


def prepare(directory, *, executables=None, python_guard=None):
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    shims, empty = directory / 'bin', directory / 'empty-apps'
    shims.mkdir()
    empty.mkdir()
    tools = {}
    for tool in ALLOWED:
        source = (executables or {}).get(tool) or shutil.which(tool)
        if not source:
            raise ValueError(f'Pinned {tool} is required for tooling metadata checks')
        # Keep the invoked basename: rustup proxies dispatch using argv[0].
        path = Path(os.path.abspath(source))
        if not path.is_file():
            raise ValueError(f'Invalid {tool} executable')
        tools[tool] = {'path': str(path), 'resolved': str(path.resolve()), 'sha256': digest(path)}
    manifest = {'version': 1, 'tools': tools, 'shim_source_sha256': source_digest()}
    (directory / 'manifest.json').write_text(json.dumps(manifest, sort_keys=True) + '\n', encoding='utf-8')
    (directory / 'audit.jsonl').write_text('', encoding='utf-8')
    script = str(Path(__file__).resolve())
    for tool in ALLOWED:
        if os.name == 'nt':
            # Batch transport is a compatibility shim, not an argument sandbox.
            command = f'@"{sys.executable}" "{script}" --dispatch "{directory}" {tool} %*\r\n'
            (shims / (tool + '.cmd')).write_text(command, encoding='utf-8', newline='')
        else:
            command = (f'#!{sys.executable}\nimport runpy, sys\n'
                       f'sys.argv = [{script!r}, "--dispatch", {str(directory)!r}, {tool!r}, *sys.argv[1:]]\n'
                       f'runpy.run_path({script!r}, run_name="__main__")\n')
            path = shims / tool
            path.write_text(command, encoding='utf-8')
            path.chmod(0o755)
    environment = {'PATH': str(shims) + os.pathsep + os.environ.get('PATH', ''),
            'TOR_TEST_BINARIES': str(empty),
            'TOR_SCENARIO_COMPILER': str(empty / ('tor-scenario.exe' if os.name == 'nt' else 'tor-scenario'))}
    if python_guard is None:
        python_guard = os.name == 'nt'
    if type(python_guard) is not bool:
        raise ValueError('Invalid Python startup guard policy')
    if python_guard:
        startup = directory / 'startup'
        startup.mkdir()
        code = ('import runpy\n'
                f'_guard = runpy.run_path({script!r})\n'
                f'_guard["install_python_guard"]({str(directory)!r}, {source_digest()!r})\n')
        (startup / 'sitecustomize.py').write_text(code, encoding='utf-8')
        manifest['python_startup'] = {'path': str(startup / 'sitecustomize.py'),
                                      'sha256': digest(startup / 'sitecustomize.py')}
        (directory / 'manifest.json').write_text(json.dumps(manifest, sort_keys=True) + '\n', encoding='utf-8')
        environment['PYTHONPATH'] = str(startup) + os.pathsep + os.environ.get('PYTHONPATH', '')
    return environment


def install_python_guard(directory, expected_source):
    """Cover reviewed Python direct compiler launches, not native grandchildren.

    Windows CreateProcess does not resolve an extensionless cargo to cargo.cmd.
    Redirect at the Python Popen boundary instead, preserving normal Popen APIs.
    Only dispatch's validated single native forwarding uses the saved original.
    """
    if source_digest() != expected_source:
        raise ValueError('Python startup guard source changed')
    directory = str(Path(directory).resolve())
    original = getattr(subprocess, '_ci_no_build_original_popen', subprocess.Popen)
    script = str(Path(__file__).resolve())

    def compiler(value):
        if not isinstance(value, (str, bytes, os.PathLike)):
            return None
        name = os.fsdecode(value).replace('\\', '/').rsplit('/', 1)[-1].lower()
        name = name.rsplit('.', 1)[0] if name.endswith(('.exe', '.cmd', '.bat')) else name
        return name if name in ALLOWED else None

    class GuardedPopen(original):
        def __init__(self, args, *positional, **kwargs):
            first = compiler(args[0]) if isinstance(args, (list, tuple)) and args else None
            override = compiler(kwargs.get('executable'))
            if len(positional) > 1:
                override = override or compiler(positional[1])
            tool = first or override
            if tool:
                arguments = list(args[1:]) if first else ['unsupported-launch-form']
                if positional or kwargs.get('shell') or kwargs.get('executable') is not None:
                    arguments = ['unsupported-launch-form']
                args = [sys.executable, script, '--dispatch', directory, tool, *arguments]
                kwargs.pop('executable', None)
                kwargs['shell'] = False
                positional = ()
            super().__init__(args, *positional, **kwargs)

    subprocess._ci_no_build_original_popen = original
    subprocess._ci_no_build_directory = directory
    subprocess.Popen = GuardedPopen


def probe(directory, environment):
    """Require actual inherited extensionless compiler interception before tests."""
    directory = Path(directory).resolve()
    manifest = strict_json((directory / 'manifest.json').read_text(encoding='utf-8'))
    if 'python_startup' not in manifest:
        return
    before = validate_audit(directory)['metadata_calls']
    code = ('import subprocess, sys; '
            f'sys.exit(125) if getattr(subprocess, "_ci_no_build_directory", None) != {str(directory)!r} else None; '
            'sys.exit(subprocess.run(["cargo", "-V"]).returncode)')
    result = subprocess.run([sys.executable, '-c', code], env={**os.environ, **environment},
                            cwd=directory, capture_output=True, text=True, timeout=30)
    if result.returncode or validate_audit(directory)['metadata_calls'] != before + 1:
        raise ValueError('Python startup guard probe failed: ' + result.stderr)


def append(directory, row):
    encoded = (json.dumps(row, sort_keys=True) + '\n').encode()
    fd = os.open(directory / 'audit.jsonl', os.O_WRONLY | os.O_APPEND)
    try:
        if os.write(fd, encoded) != len(encoded):
            raise OSError('Incomplete no-build audit write')
    finally:
        os.close(fd)


def dispatch(directory, tool, arguments):
    directory = Path(directory)
    manifest = strict_json((directory / 'manifest.json').read_text(encoding='utf-8'))
    record = manifest['tools'].get(tool)
    permitted = tool in ALLOWED and tuple(arguments) in ALLOWED[tool]
    if record is None or manifest.get('shim_source_sha256') != source_digest():
        permitted = False
    elif str(Path(record['path']).resolve()) != record['resolved'] or digest(record['path']) != record['sha256']:
        permitted = False
    invocation = uuid.uuid4().hex
    append(directory, {'event': 'request', 'invocation': invocation, 'tool': tool, 'arguments': arguments,
                       'permitted': permitted, 'tool_sha256': record['sha256'] if record else None})
    if not permitted:
        print(f'No-build tooling guard rejected {tool} {arguments!r}', file=sys.stderr)
        return 126
    env = dict(os.environ, CARGO_NET_OFFLINE='true')
    command = [record['path'], *arguments]
    original = getattr(subprocess, '_ci_no_build_original_popen', None)
    if original is None:
        result = subprocess.run(command, env=env, check=False)
    else:
        # This is the sole internal bypass: command/hash/arguments were checked
        # above. No environment flag can disable interception in test children.
        with original(command, env=env) as child:
            child.communicate()
            result = subprocess.CompletedProcess(command, child.returncode)
    append(directory, {'event': 'completion', 'invocation': invocation, 'tool': tool, 'arguments': arguments,
                       'returncode': result.returncode})
    return result.returncode


def validate_audit(directory, *, check_tools=True):
    """Check complete evidence; live runners also verify their tool files.

    A foreign-OS aggregate may set check_tools=False only after binding these
    files to its successful same-run lane receipt. That is evidence validation,
    not a claim that this host executed or independently attested those tools.
    """
    if type(check_tools) is not bool:
        raise ValueError('Invalid tool-check policy')
    directory = Path(directory)
    manifest = strict_json((directory / 'manifest.json').read_text(encoding='utf-8'))
    if type(manifest.get('version')) is not int or manifest['version'] != 1 or manifest.get('shim_source_sha256') != source_digest():
        raise ValueError('No-build guard changed during execution')
    startup = manifest.get('python_startup')
    if startup is not None:
        if (not isinstance(startup, dict) or set(startup) != {'path', 'sha256'}
                or not all(isinstance(startup[key], str) and startup[key] for key in startup)
                or len(startup['sha256']) != 64):
            raise ValueError('Invalid Python startup identity')
        if check_tools and digest(startup['path']) != startup['sha256']:
            raise ValueError('Python startup guard changed')
    raw = (directory / 'audit.jsonl').read_bytes()
    if len(raw) > 4 * 1024 * 1024 or (raw and not raw.endswith(b'\n')):
        raise ValueError('Incomplete or oversized no-build audit')
    requests, completions = {}, {}
    for line in raw.splitlines():
        row = strict_json(line)
        tool, arguments = row.get('tool'), row.get('arguments')
        invocation = row.get('invocation')
        if not isinstance(invocation, str) or len(invocation) != 32 or any(char not in '0123456789abcdef' for char in invocation):
            raise ValueError('Invalid invocation identity')
        if not isinstance(arguments, list) or not all(isinstance(arg, str) for arg in arguments):
            raise ValueError('Invalid no-build arguments')
        key = (tool, tuple(arguments))
        if tool not in ALLOWED or tuple(arguments) not in ALLOWED[tool]:
            raise ValueError('Unexpected compiler action in tooling lane')
        if row.get('event') == 'request':
            if row.get('permitted') is not True or row.get('tool_sha256') != manifest['tools'][tool]['sha256']:
                raise ValueError('Compiler guard denied an invocation')
            if invocation in requests:
                raise ValueError('Duplicate metadata request')
            requests[invocation] = key
        elif row.get('event') == 'completion':
            if type(row.get('returncode')) is not int or row['returncode'] != 0:
                raise ValueError('Metadata compiler invocation failed')
            if invocation in completions:
                raise ValueError('Duplicate metadata completion')
            completions[invocation] = key
        else:
            raise ValueError('Unknown no-build audit event')
    if requests != completions:
        raise ValueError('Unfinished metadata compiler invocation')
    if set(manifest.get('tools', {})) != set(ALLOWED):
        raise ValueError('Incomplete tool identities')
    for record in manifest['tools'].values():
        if (not all(isinstance(record.get(key), str) and record[key] for key in ('path', 'resolved', 'sha256'))
                or len(record['sha256']) != 64
                or any(char not in '0123456789abcdef' for char in record['sha256'])):
            raise ValueError('Invalid tool identity')
        if check_tools and (str(Path(record['path']).resolve()) != record['resolved'] or digest(record['path']) != record['sha256']):
            raise ValueError('Compiler identity changed during tooling checks')
    return {'metadata_calls': len(requests), 'build_calls': 0}


if __name__ == '__main__':
    if len(sys.argv) < 4 or sys.argv[1] != '--dispatch':
        raise SystemExit('Use prepare() from the selected tooling runner')
    raise SystemExit(dispatch(Path(sys.argv[2]), sys.argv[3], sys.argv[4:]))
