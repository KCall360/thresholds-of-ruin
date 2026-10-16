"""Reviewed expectation-only leaves, never general dependency inference.

All unclassified changes fall back. The registry is a code-review contract: new
outside consumers must update it; pairwise tree equality cannot detect a missed
review obligation. Original provenance and resume checks are never rewritten.
"""
import ast
import hashlib
import io
import json
from pathlib import Path
import re
import tokenize

from ci_qualification import require

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / 'scripts/ci_reuse_registry.json'
ASSERTIONS = {'assertEqual': 1, 'assertCountEqual': 1, 'assertIn': 0, 'assertNotIn': 0}
ROUTE_ENVIRONMENT = ('TOR_EARNED_ROUTE_ARTIFACTS', 'TOR_EARNED_ROUTE_TIMING',
                     'TOR_EARNED_ROUTE_PRODUCTION_SOURCE', 'TOR_EARNED_ROUTE_CANDIDATE_SOURCE')


def syntax(raw):
    require(isinstance(raw, bytes) and 0 < len(raw) <= 128 * 1024, 'unbounded leaf source')
    require(tokenize.detect_encoding(io.BytesIO(raw).readline)[0] == 'utf-8', 'unsupported source encoding')
    return ast.parse(raw)


def fingerprint(tree):
    return hashlib.sha256(ast.dump(tree, include_attributes=False).encode()).hexdigest()


def bounded(value, depth=0, budget=None):
    budget = [4096] if budget is None else budget
    budget[0] -= 1
    require(depth <= 16 and budget[0] >= 0, "expanded expected value budget")
    require(type(value) in (str, int, bool, type(None), list, tuple, dict), 'nonprimitive expected value')
    if type(value) is int: require(value.bit_length() <= 64, 'unbounded expected integer')
    if isinstance(value, str): require(len(value) <= 1024, 'unbounded expected string')
    if isinstance(value, (list, tuple, dict)):
        require(len(value) <= 256, 'unbounded expected container')
        for item in (list(value) + list(value.values()) if isinstance(value, dict) else value): bounded(item, depth + 1, budget)
    return value


def expression_syntax(node, names, depth=0):
    """Validate dead branches too: evaluation is not a syntax whitelist."""
    require(depth <= 16, 'expected syntax depth')
    child = lambda item, scope=names: expression_syntax(item, scope, depth + 1)
    if isinstance(node, ast.Name):
        require(isinstance(node.ctx, ast.Load) and node.id in names, 'unknown expected binding')
    elif isinstance(node, ast.Call):
        require(isinstance(node.func, ast.Name) and node.func.id == 'len' and 'len' not in names
                and len(node.args) == 1 and not node.keywords, 'unknown expected call')
        child(node.args[0])
    elif isinstance(node, ast.ListComp):
        require(len(node.generators) == 1, 'multiple expected generators')
        generator = node.generators[0]
        require(isinstance(generator.target, ast.Name) and not generator.is_async
                and generator.target.id not in names | {'self', 'len'}, 'unknown expected generator')
        child(generator.iter)
        for item in [node.elt, *generator.ifs]: child(item, names | {generator.target.id})
    else:
        require(type(node) in (ast.Constant, ast.List, ast.Tuple, ast.Dict, ast.Subscript,
                ast.UnaryOp, ast.UAdd, ast.USub, ast.BinOp, ast.Add, ast.Sub, ast.Compare, ast.Eq, ast.NotEq,
                ast.JoinedStr, ast.FormattedValue, ast.Load), 'unclassified expected syntax')
        if isinstance(node, ast.FormattedValue):
            require(node.conversion == -1 and node.format_spec is None, 'custom expected formatting')
        if isinstance(node, ast.Constant): bounded(node.value)
        for item in ast.iter_child_nodes(node): child(item)


def pure(node, names, depth=0, budget=None):
    """Interpret only bounded built-in syntax; never execute user Python."""
    if budget is None: expression_syntax(node, set(names))
    budget = [4096] if budget is None else budget
    budget[0] -= 1
    require(budget[0] >= 0 and depth <= 16 and sum(1 for _ in ast.walk(node)) <= 256, 'unbounded expected expression')
    child = lambda item, scope=names: pure(item, scope, depth + 1, budget)
    if isinstance(node, ast.Constant): return bounded(node.value)
    if isinstance(node, ast.Name):
        require(node.id in names, 'unknown expected binding')
        return bounded(names[node.id])
    if isinstance(node, (ast.List, ast.Tuple)):
        values = [child(item) for item in node.elts]
        return bounded(tuple(values) if isinstance(node, ast.Tuple) else values)
    if isinstance(node, ast.Dict):
        require(all(key is not None for key in node.keys), 'expanded expected mapping')
        keys = [child(key) for key in node.keys]
        require(all(type(key) in (str, int, bool, type(None)) for key in keys), 'unknown expected key')
        return bounded(dict(zip(keys, (child(value) for value in node.values))))
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == 'len':
        require(len(node.args) == 1 and not node.keywords and 'len' not in names, 'shadowed or expanded len')
        value = child(node.args[0])
        require(type(value) in (str, tuple, list, dict), 'len of noncontainer')
        return len(value)
    if isinstance(node, ast.Subscript):
        value, key = child(node.value), child(node.slice)
        require(type(value) in (tuple, list, dict, str) and type(key) in (int, str), 'unknown expected indexing')
        return bounded(value[key])
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
        value = child(node.operand)
        require(type(value) is int, 'unknown unary expected value')
        return bounded(-value if isinstance(node.op, ast.USub) else value)
    if isinstance(node, ast.BinOp) and isinstance(node.op, (ast.Add, ast.Sub)):
        left, right = child(node.left), child(node.right)
        require(type(left) is type(right) and (type(left) is int or
                isinstance(node.op, ast.Add) and type(left) in (str, tuple, list)), 'unknown expected arithmetic')
        return bounded(left + right if isinstance(node.op, ast.Add) else left - right)
    if isinstance(node, ast.Compare):
        require(len(node.ops) == len(node.comparators) == 1 and isinstance(node.ops[0], (ast.Eq, ast.NotEq)),
                'unknown expected comparison')
        equal = child(node.left) == child(node.comparators[0])
        return not equal if isinstance(node.ops[0], ast.NotEq) else equal
    if isinstance(node, ast.ListComp):
        require(len(node.generators) == 1, 'multiple expected generators')
        generator = node.generators[0]
        require(isinstance(generator.target, ast.Name) and not generator.is_async
                and generator.target.id not in names and generator.target.id not in ('self', 'len'),
                'shadowed expected generator')
        values = child(generator.iter)
        require(type(values) in (tuple, list) and len(values) <= 256, 'unknown expected iterable')
        result = []
        for value in values:
            scope = {**names, generator.target.id: value}
            conditions = [child(condition, scope) for condition in generator.ifs]
            require(all(type(condition) is bool for condition in conditions), 'unknown expected filter')
            if all(conditions): result.append(child(node.elt, scope))
        return bounded(result)
    if isinstance(node, ast.JoinedStr):
        pieces = []
        for value in node.values:
            if isinstance(value, ast.Constant):
                require(type(value.value) is str, 'unknown expected string')
                pieces.append(value.value)
            else:
                require(isinstance(value, ast.FormattedValue) and value.conversion == -1 and value.format_spec is None,
                        'custom expected formatting')
                formatted = child(value.value)
                require(type(formatted) in (str, int, bool, type(None)), 'nonprimitive formatted value')
                pieces.append(str(formatted))
        return bounded(''.join(pieces))
    raise ValueError('unclassified expected expression')


def immutable_provider(raw, *, name='SPECIES'):
    tree = syntax(raw)
    bindings = [node for node in tree.body if isinstance(node, ast.Assign)
                and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name) and node.targets[0].id == name]
    require(len(bindings) == 1 and isinstance(bindings[0].value, ast.Tuple), 'provider is not a literal tuple')
    value = pure(bindings[0].value, {})
    require(value and all(type(item) is str and re.fullmatch(r'[a-z][a-z0-9_]{0,63}', item) for item in value)
            and len(value) == len(set(value)), 'invalid immutable provider tuple')
    bindings[0].value = ast.Constant(value='<reviewed immutable tuple>')
    return value, fingerprint(tree)


def normalize_leaf(raw, entry, immutable):
    tree = syntax(raw)
    optional = entry.get('optional_imports', {})
    imports, imported_names, optional_seen = [], set(), set()
    for node in tree.body:
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            imported_names.update(alias.asname or alias.name.split('.')[0] for alias in node.names)
        if (isinstance(node, ast.ImportFrom) and not node.level and len(node.names) == 1
                and node.names[0].asname is None and optional.get(node.names[0].name) == node.module):
            name = node.names[0].name
            require(name not in optional_seen, 'duplicate optional import')
            optional_seen.add(name)
            continue
        imports.append(node)
    tree.body = imports
    require('len' not in imported_names, 'shadowed len import')
    sites, methods = [], []
    for cls in (node for node in tree.body if isinstance(node, ast.ClassDef)):
        for function in (node for node in cls.body if isinstance(node, ast.FunctionDef)):
            owner = cls.name + '.' + function.name
            if function.name.startswith('test_'): methods.append(owner)
            if owner not in entry['acceptance_functions']: continue
            require(not function.decorator_list and not function.args.defaults and not function.args.kw_defaults,
                    'unsupported acceptance declaration')
            # Nested functions remain completely fixed, including setup/tool guards.
            nodes = []
            def visit(node):
                nodes.append(node)
                for child in ast.iter_child_nodes(node):
                    if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef, ast.Lambda)):
                        continue
                    visit(child)
            visit(function)
            require(not any(isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                            and node.func.id in ('locals', 'globals', 'eval', 'exec', 'vars') for node in nodes),
                    'reflective acceptance function')
            parameters = {arg.arg for arg in (*function.args.posonlyargs, *function.args.args, *function.args.kwonlyargs)}
            stores = [node.id for node in nodes if isinstance(node, ast.Name) and isinstance(node.ctx, (ast.Store, ast.Del))]
            require('len' not in stores and 'len' not in parameters, 'shadowed len local')
            names = {name: value for name, value in immutable.items() if name in imported_names}
            assignments = [node for node in nodes if isinstance(node, ast.Assign) and len(node.targets) == 1
                           and isinstance(node.targets[0], ast.Name) and node.targets[0].id.startswith('expected_')]
            accepted_assignments = []
            for assignment in sorted(assignments, key=lambda node: node.lineno):
                name = assignment.targets[0].id
                require(stores.count(name) == 1 and name not in parameters | imported_names | names.keys(), 'expected binding shadow/overwrite')
                try: value = pure(assignment.value, names)
                except (ValueError, TypeError, KeyError, IndexError): continue
                names[name] = value
                accepted_assignments.append(assignment)
            accepted = []
            for node in nodes:
                if not isinstance(node, ast.Expr) or not isinstance(node.value, ast.Call): continue
                call = node.value
                if not (isinstance(call.func, ast.Attribute) and isinstance(call.func.value, ast.Name)
                        and call.func.value.id == 'self' and call.func.attr in ASSERTIONS): continue
                index = ASSERTIONS[call.func.attr]
                if len(call.args) < 2 or call.keywords: continue
                try: pure(call.args[index], names)
                except (ValueError, TypeError, KeyError, IndexError): continue
                require(not any(isinstance(other, ast.stmt) and other is not node
                                and other.lineno <= node.lineno <= other.end_lineno
                                and not any(child is node for child in ast.walk(other)) for other in ast.walk(tree)),
                        'ambiguous acceptance statement line')
                accepted.append((call, index))
                sites.append({'function': owner, 'line': node.lineno})
            allowed = {id(node) for call, index in accepted for node in ast.walk(call.args[index])}
            allowed.update(id(node) for assignment in accepted_assignments for node in ast.walk(assignment.value))
            for assignment in accepted_assignments:
                name = assignment.targets[0].id
                loads = [node for node in ast.walk(function) if isinstance(node, ast.Name) and node.id == name and isinstance(node.ctx, ast.Load)]
                require(loads and all(id(node) in allowed and node.lineno > assignment.lineno for node in loads), 'expected local escapes or precedes binding')
            # All stores, aliases and shadowing, including nested scopes, count.
            for assignment in accepted_assignments:
                name = assignment.targets[0].id
                require(sum(isinstance(node, ast.Name) and node.id == name and isinstance(node.ctx, (ast.Store, ast.Del))
                            for node in ast.walk(function)) == 1, 'nested expected binding shadow')
            for call, index in accepted: call.args[index] = ast.Constant(value='<reviewed expected value>')
            removed = {id(node) for node in accepted_assignments}
            for node in ast.walk(function):
                for field, value in ast.iter_fields(node):
                    if isinstance(value, list): setattr(node, field, [item for item in value if id(item) not in removed])
    require(sorted(methods) == entry['methods'], 'leaf method inventory changed')
    return {'skeleton': fingerprint(tree), 'sites': sites, 'methods': methods}


def validate_trees(source, target, source_bytes, target_bytes, *, registry=None):
    """Tree entries are independently API-bound path -> [mode, blob SHA]."""
    registry = json.loads(REGISTRY.read_text()) if registry is None else registry
    require(registry['schema_version'] == 1 and set(source) == set(target), 'added or deleted inputs')
    require(len(source) == len({path.casefold() for path in source}), 'ambiguous input case')
    require(all(type(entry) is list and len(entry) == 2 and entry[0] in ('100644', '100755')
                and isinstance(entry[1], str) and re.fullmatch(r'[0-9a-f]{40}', entry[1])
                for inputs in (source, target) for entry in inputs.values()), 'unsupported input entry')
    changed = [path for path in source if source[path] != target[path]]
    require(0 < len(changed) <= 3 and set(changed) <= registry['leaves'].keys(), 'unreviewed changed paths')
    for path, expected in registry['owner_blobs'].items():
        require(source.get(path) == target.get(path) == ['100644', expected], 'reviewed consumer owner drift')
    for path in changed:
        require(source[path][0] == target[path][0] == '100644', 'nonordinary changed leaf')
    provider = 'scripts/rogue_roster_package.py'
    values, shape = immutable_provider(target_bytes(provider))
    require(shape == registry['provider_skeleton'], 'immutable provider code changed')
    for path, expected in registry['import_blobs'].items():
        require(source.get(path) == target.get(path) == ['100644', expected], 'import-time owner drift')
    result = {}
    for path in changed:
        entry = registry['leaves'][path]
        immutable = {name: values for name in entry['immutable_names']}
        old, new = (normalize_leaf(reader(path), entry, immutable) for reader in (source_bytes, target_bytes))
        require(old['skeleton'] == new['skeleton'] == entry['skeleton'], 'leaf actions or boundaries changed')
        result[path] = {'old_sites': old['sites'], 'methods': sorted(path.split('/')[-1][:-3] + '.' + name for name in new['methods'])}
    return result
