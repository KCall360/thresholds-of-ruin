"""Correlate opt-in server/client/reader diagnostics by request identity.

Cross-process boundaries use the same host's wall clock; durations use monotonic
clocks. Diagnostic stderr/stdout and OS scheduling are included, not subtracted.
"""
import argparse
import json
import math
from pathlib import Path


def events(path):
    text = path.read_text()
    lines = text.splitlines()
    # Processes are killed when a run ends, which can cut off the line being
    # written: a line without its newline is incomplete, never evidence.
    if lines and not text.endswith("\n"):
        lines.pop()
    rows = [json.loads(line) for line in lines if line.startswith('{')]
    return [r for r in rows if r.get('timing_version') == 1]


def diagnostic_write_costs(rows):
    client = [r for r in rows if r['event'] in ('client_request','client_request_sent','client_ack')]
    costs = {}
    for previous, following in zip(client, client[1:]):
        if 'previous_timing_write_ms' in following:
            value = following['previous_timing_write_ms']
            if not (type(value) in (int,float) and math.isfinite(value) and value >= 0):
                raise ValueError('Invalid diagnostic write duration: expected a finite nonnegative number')
            costs[(previous['request_id'], previous['event'])] = value
    return costs


def event_positions(rows, kinds):
    """Index required evidence without silently replacing an ambiguous record."""
    positions = {}
    for index, row in enumerate(rows):
        if row['event'] not in kinds or not row.get('request_id'):
            continue
        key = (row['request_id'], row['event'])
        if key in positions:
            raise ValueError(f'Duplicate {row["event"]} diagnostic for request {row["request_id"]!r}')
        positions[key] = index
    return positions


def client_event_order(positions, request, *, headless=False):
    kinds = ('client_request', 'client_request_sent', 'client_ack')
    if headless:
        kinds += ('headless_report',)
    indices = [positions[(request, kind)] for kind in kinds]
    if headless and indices[-1] < indices[0]:
        raise ValueError('Headless acknowledgement report precedes its request')
    # These records are emitted sequentially by the client, unlike server
    # handling/send diagnostics from independent tasks. Compare log positions,
    # never wall timestamps: clock adjustments remain valid signed evidence.
    if indices != sorted(indices):
        raise ValueError(f'Invalid client diagnostic order for request {request!r}')
    return indices


def server_events(path):
    rows = events(path)
    dropped = 0
    for row in rows:
        if row['event'] == 'server_diagnostics_dropped':
            count = row.get('dropped')
            if type(count) is not int or count <= 0:
                raise ValueError('Invalid server diagnostic loss record')
            dropped += count
    if dropped:
        raise ValueError(f'Server diagnostics dropped {dropped} records; correlation is incomplete')
    # A server log combines clients. Setup requests can legitimately share an
    # identity across them; retain every row and require uniqueness only when
    # resolving a measured request, rather than overwriting or rejecting setup.
    indexed = {}
    for row in rows:
        indexed.setdefault((row['request_id'], row['event']), []).append(row)
    return indexed


def measured_server_event(indexed, request, kind):
    candidates = indexed[(request, kind)]
    if len(candidates) != 1:
        raise ValueError(f'Duplicate {kind} diagnostic for measured request {request!r}')
    return candidates[0]


def measured_server_pair(indexed, request):
    handled, sent = [measured_server_event(indexed, request, kind)
                     for kind in ('server_handled', 'server_ack_sent')]
    # Both producers stamp the same transport connection ID. It is not the
    # driver's actor index; unrelated setup requests may still share an ID.
    clients = [row.get('client') for row in (handled, sent)]
    if (any(type(client) is not int or not 0 <= client < 2**64 for client in clients)
            or clients[0] != clients[1]):
        raise ValueError(f'Invalid server client identity for measured request {request!r}')
    return handled, sent


def correlate_ack(directory, result):
    directory = Path(directory)
    server = server_events(directory/'server.stderr.log')
    clients = {actor: events(directory/f'actor-{actor}.stderr.log') for actor in range(1,result['actors']+1)}
    positions = {actor: event_positions(rows, (
        'client_request', 'client_request_sent', 'client_ack', 'headless_report'))
        for actor, rows in clients.items()}
    writes = {actor:diagnostic_write_costs(rows) for actor,rows in clients.items()}
    output = []
    for sample in result['samples']:
        if sample['expected'] == 'blocked':
            continue
        request = sample['request_id']
        client = clients[sample['actor']]
        start_index, sent_index, ack_index, report_index = client_event_order(
            positions[sample['actor']], request, headless=True)
        start, sent, ack, report = [client[index] for index in
                                    (start_index, sent_index, ack_index, report_index)]
        # The client log establishes this causal window. Host wall-clock drift
        # must not exclude its matching report or include unrelated reports.
        report_max = max(client[index]['duration_ms']
                         for index in range(start_index, report_index + 1)
                         if client[index]['event'] == 'headless_report')
        handled, sent_ack = measured_server_pair(server, request)
        row = dict(request_id=request, actor=sample['actor'], label=sample['label'],
                   request_diagnostic_write_ms=writes[sample['actor']].get((request,'client_request')),
                   sent_diagnostic_write_ms=writes[sample['actor']].get((request,'client_request_sent')),
                   ack_diagnostic_write_ms=writes[sample['actor']].get((request,'client_ack')),
                   driver_ack_ms=sample['request_to_ack_ms'], reader_ack_ms=sample['request_to_ack_line_ms'],
                   driver_before_client_ms=(start['unix_ns']-sample['input_unix_ns'])/1e6,
                   client_request_to_ack_ms=(ack['unix_ns']-start['unix_ns'])/1e6,
                   client_send_ms=sent['duration_ms'], server_lock_ms=handled['lock_ms'],
                   server_handle_ms=handled['duration_ms'],
                   server_handle_to_ack_sent_ms=(sent_ack['unix_ns']-handled['unix_ns'])/1e6,
                   server_ack_to_client_ack_ms=(ack['unix_ns']-sent_ack['unix_ns'])/1e6,
                   headless_ack_report_ms=report['duration_ms'],
                   headless_report_max_ms=report_max,
                   client_ack_to_reader_ms=(sample['ack_line_unix_ns']-ack['unix_ns'])/1e6,
                   driver_queue_ms=sample['request_to_ack_ms']-sample['request_to_ack_line_ms'])
        # Cross-process clocks may drift or adjust; a receiver can also run
        # before a sender returns. Preserve signed diagnostics without clamping.
        if not all((v is None and k.endswith('_diagnostic_write_ms')) or
                   (type(v) in (int,float) and math.isfinite(v))
                   for k,v in row.items() if k.endswith('_ms')):
            raise ValueError('Invalid acknowledgement timing: expected finite numeric evidence')
        output.append(row)
    return output


def correlate_native(directory, result):
    directory = Path(directory)
    server = server_events(directory/'server.stderr.log')
    client = events(directory/'ascii.stderr.log')
    positions = event_positions(client, ('client_request', 'client_request_sent', 'client_ack'))
    requests = {}
    for row in client:
        if row['event'] == 'client_request':
            candidates = requests.setdefault(row['revision'], [])
            # Startup control/query requests can share an Act's revision. Only
            # explicit producer classification may exclude them; unclassified
            # legacy requests remain candidates and must still be unambiguous.
            if 'request_kind' in row:
                if row['request_kind'] not in ('act', 'non_action'):
                    raise ValueError('Invalid native request_kind diagnostic')
                if row['request_kind'] == 'non_action':
                    continue
            candidates.append(row)
    acknowledgements = {r['request_id']:r for r in client if r['event']=='client_ack'}
    sends = {r['request_id']:r for r in client if r['event']=='client_request_sent'}
    write_cost = diagnostic_write_costs(client)
    # A frame reports the duration of the preceding diagnostic output call.
    # Recover that later sample rather than assigning the previous call to the
    # frame being measured. This read happens offline, after the workload.
    action_frames, report_cost, frame_times, report_parts = {}, {}, {}, {}
    following_frames = {}
    previous_frame = None
    with (directory/'ascii.stdout.jsonl').open() as stream:
        for line in stream:
            try:
                frame = json.loads(line)
            except json.JSONDecodeError:
                # Process cleanup may interrupt an unmeasured final report.
                # Completed action coverage was already checked by the driver.
                if not line.endswith('\n'):
                    break
                raise
            if frame['frame'] in frame_times:
                raise ValueError(f'Duplicate native frame diagnostic {frame["frame"]!r}')
            if previous_frame is not None:
                following_frames[previous_frame] = frame['frame']
                report_cost[previous_frame] = frame['profile']['previous_report_ms']
                report_parts[previous_frame] = {k:frame['profile'].get('previous_report_'+k+'_ms') for k in ('encode','write')}
            previous_frame = frame['frame']
            frame_times[frame['frame']] = frame.get('presented_unix_ns')
            if frame.get('input_done') in ('up','down','left','right','ascend','descend') and not frame['busy']:
                action_frames.setdefault(int(frame['state']['revision'])-1, []).append(frame['frame'])
    output = []
    for sample in result['samples']:
        # The complete single-actor traversal advances exactly one revision per
        # ordinary action. Modal door selection sends no request.
        candidates = requests[sample['index']]
        if len(candidates) != 1:
            raise ValueError(f'Ambiguous native request revision {sample["index"]!r}')
        request = candidates[0]
        request_id = request['request_id']
        client_event_order(positions, request_id)
        handled, sent = measured_server_pair(server, request_id)
        ack = acknowledgements[request_id]
        if 'presented_frame' in sample:
            frame_id = sample['presented_frame']
        else:
            candidates = action_frames[sample['index']]
            if len(candidates) != 1:
                raise ValueError(f'Ambiguous native frame revision {sample["index"]!r}')
            frame_id = candidates[0]
        if frame_id not in frame_times:
            raise ValueError(f'Missing selected native frame diagnostic {frame_id!r}')
        following = following_frames.get(frame_id)
        # Dirty presentations increment the producer's frame ID exactly once.
        # A later parsed row is not necessarily the true next report if the
        # capture has a gap or reorder. Validate only this measured boundary.
        # A final frame may lack its optional report cost, unless its actual
        # successor was already observed earlier (a known reorder, not a tail).
        if ((following is not None and following != frame_id + 1)
                or (following is None and frame_id + 1 in frame_times)):
            raise ValueError(f'Invalid selected native frame successor for {frame_id!r}')
        presented = frame_times[frame_id]
        output.append(dict(index=sample['index'], request_id=request_id,
            presentation_ms=sample['request_to_presentation_ms'],
            input_to_client_request_ms=(request['unix_ns']-sample['input_unix_ns'])/1e6,
            client_send_ms=sends[request_id]['duration_ms'],
            request_diagnostic_write_ms=write_cost.get((request_id,'client_request')),
            sent_diagnostic_write_ms=write_cost.get((request_id,'client_request_sent')),
            ack_diagnostic_write_ms=write_cost.get((request_id,'client_ack')),
            server_handle_ms=handled['duration_ms'], server_lock_ms=handled['lock_ms'],
            server_handle_to_ack_sent_ms=(sent['unix_ns']-handled['unix_ns'])/1e6,
            server_ack_to_client_ack_ms=(ack['unix_ns']-sent['unix_ns'])/1e6,
            client_ack_to_frame_reader_ms=(sample['line_unix_ns']-ack['unix_ns'])/1e6,
            client_ack_to_presented_ms=None if presented is None else (presented-ack['unix_ns'])/1e6,
            presented_to_reader_ms=None if presented is None else (sample['line_unix_ns']-presented)/1e6,
            measured_frame_encode_ms=report_parts.get(frame_id,{}).get('encode'),
            measured_frame_write_ms=report_parts.get(frame_id,{}).get('write'),
            reader_work_ms=sample['reader_work_ms'], queue_delay_ms=sample['queue_delay_ms'],
            max_previous_report_ms=max(p['previous_report_ms'] for p in sample['intermediate_profiles']),
            measured_frame_report_ms=report_cost.get(frame_id)))
    optional = {'request_diagnostic_write_ms','sent_diagnostic_write_ms','ack_diagnostic_write_ms',
                'client_ack_to_presented_ms','presented_to_reader_ms','measured_frame_encode_ms',
                'measured_frame_write_ms','measured_frame_report_ms'}
    if not all((v is None and k in optional) or (type(v) in (int,float) and math.isfinite(v))
               for row in output for k,v in row.items() if k.endswith('_ms')):
        raise ValueError('Invalid native timing: expected finite numeric evidence')
    return output


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory',type=Path)
    parser.add_argument('--native',action='store_true')
    parser.add_argument('--output',type=Path,required=True)
    args = parser.parse_args()
    result = json.loads((args.directory/'result.json').read_text())
    rows = (correlate_native if args.native else correlate_ack)(args.directory,result)
    args.output.write_text(json.dumps(rows,indent=2))
    print(f'Correlated {len(rows)} accepted acknowledgements.')
