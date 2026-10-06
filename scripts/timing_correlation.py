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
            assert type(value) in (int,float) and math.isfinite(value) and value >= 0
            costs[(previous['request_id'], previous['event'])] = value
    return costs


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
    return {(row['request_id'], row['event']): row for row in rows}


def correlate_ack(directory, result):
    directory = Path(directory)
    server = server_events(directory/'server.stderr.log')
    clients = {actor: events(directory/f'actor-{actor}.stderr.log') for actor in range(1,result['actors']+1)}
    indexed = {actor:{(r['request_id'],r['event']):r for r in rows if r.get('request_id')}
               for actor,rows in clients.items()}
    writes = {actor:diagnostic_write_costs(rows) for actor,rows in clients.items()}
    output = []
    for sample in result['samples']:
        if sample['expected'] == 'blocked':
            continue
        request = sample['request_id']
        client = indexed[sample['actor']]
        start, sent, ack, report = [client[(request,k)] for k in ('client_request','client_request_sent','client_ack','headless_report')]
        handled, sent_ack = [server[(request,k)] for k in ('server_handled','server_ack_sent')]
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
                   headless_report_max_ms=max(r['duration_ms'] for r in clients[sample['actor']]
                       if r['event']=='headless_report' and sample['input_unix_ns'] <= r['unix_ns'] <= report['unix_ns']),
                   client_ack_to_reader_ms=(sample['ack_line_unix_ns']-ack['unix_ns'])/1e6,
                   driver_queue_ms=sample['request_to_ack_ms']-sample['request_to_ack_line_ms'])
        # Tiny negative send/receive deltas can occur because the receiver runs
        # before the sender returns from send. Keep them instead of clamping.
        assert all((v is None and k.endswith('_diagnostic_write_ms')) or
                   (type(v) in (int,float) and math.isfinite(v))
                   for k,v in row.items() if k.endswith('_ms'))
        output.append(row)
    return output


def correlate_native(directory, result):
    directory = Path(directory)
    server = server_events(directory/'server.stderr.log')
    client = events(directory/'ascii.stderr.log')
    requests = {r['revision']:r for r in client if r['event']=='client_request'}
    acknowledgements = {r['request_id']:r for r in client if r['event']=='client_ack'}
    sends = {r['request_id']:r for r in client if r['event']=='client_request_sent'}
    write_cost = diagnostic_write_costs(client)
    # A frame reports the duration of the preceding diagnostic output call.
    # Recover that later sample rather than assigning the previous call to the
    # frame being measured. This read happens offline, after the workload.
    action_frames, report_cost, frame_times, report_parts = {}, {}, {}, {}
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
            if previous_frame is not None:
                report_cost[previous_frame] = frame['profile']['previous_report_ms']
                report_parts[previous_frame] = {k:frame['profile'].get('previous_report_'+k+'_ms') for k in ('encode','write')}
            previous_frame = frame['frame']
            frame_times[frame['frame']] = frame.get('presented_unix_ns')
            if frame.get('input_done') in ('up','down','left','right','ascend','descend') and not frame['busy']:
                action_frames[int(frame['state']['revision'])-1] = frame['frame']
    output = []
    for sample in result['samples']:
        # The complete single-actor traversal advances exactly one revision per
        # ordinary action. Modal door selection sends no request.
        request = requests[sample['index']]
        request_id = request['request_id']
        handled = server[(request_id,'server_handled')]
        sent = server[(request_id,'server_ack_sent')]
        ack = acknowledgements[request_id]
        frame_id = (sample['presented_frame'] if 'presented_frame' in sample
                    else action_frames[sample['index']])
        presented = frame_times.get(frame_id)
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
    assert all((v is None and k in optional) or (type(v) in (int,float) and math.isfinite(v))
               for row in output for k,v in row.items() if k.endswith('_ms'))
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
