#!/usr/bin/env python3
"""fixture.py: writes fixtures/, an invented tracker's answers in Jira's and Trello's REST shapes, for pictures that
show no one's real work and for the tests (ARTOO_TASKS_FIXTURE=fixtures). The board, its issues and its people are
the Design repository's (design/explore/tasks/tasks.js). Run it again after changing it; the files are kept in git."""
import json, os, shutil, struct, zlib

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, '..', 'fixtures')
JIRA = 'acme.atlassian.net'
SITE = 'https://' + JIRA

PEOPLE = {
    'you': ('u-you', 'Avery Quinn'),
    'juniper': ('u-juniper', 'Juniper Hale'),
    'kestrel': ('u-kestrel', 'Kestrel Okafor'),
    'ember': ('u-ember', 'Ember Lind'),
}
def person(who):
    if not who:
        return None
    aid, name = PEOPLE[who]
    return {'accountId': aid, 'displayName': name, 'active': True,
            'avatarUrls': {'48x48': ''}, 'emailAddress': ''}

STATES = {
    'todo': {'id': '10000', 'name': 'To Do', 'statusCategory': {'id': 2, 'key': 'new', 'name': 'To Do'}},
    'prog': {'id': '3', 'name': 'In Progress', 'statusCategory': {'id': 4, 'key': 'indeterminate', 'name': 'In Progress'}},
    'rev': {'id': '10001', 'name': 'In Review', 'statusCategory': {'id': 4, 'key': 'indeterminate', 'name': 'In Progress'}},
    'done': {'id': '10002', 'name': 'Done', 'statusCategory': {'id': 3, 'key': 'done', 'name': 'Done'}},
}
TYPES = {
    'bug': {'id': '10004', 'name': 'Bug', 'subtask': False, 'hierarchyLevel': 0},
    'story': {'id': '10001', 'name': 'Story', 'subtask': False, 'hierarchyLevel': 0},
    'task': {'id': '10002', 'name': 'Task', 'subtask': False, 'hierarchyLevel': 0},
    'sub': {'id': '10003', 'name': 'Sub-task', 'subtask': True, 'hierarchyLevel': -1},
    'epic': {'id': '10000', 'name': 'Epic', 'subtask': False, 'hierarchyLevel': 1},
}
PRI = {'highest': '1', 'high': '2', 'medium': '3', 'low': '4', 'lowest': '5'}
def priority(p):
    return {'id': PRI[p], 'name': p.capitalize()}

SPRINT = {'id': 34, 'name': 'Sprint 34', 'state': 'active', 'startDate': '2026-11-03T09:00:00.000Z',
          'endDate': '2026-11-14T17:00:00.000Z', 'originBoardId': 7, 'goal': 'Refunds that add up'}
NEXT_SPRINT = {'id': 35, 'name': 'Sprint 35', 'state': 'future', 'originBoardId': 7}
EPICS = [
    {'key': 'PAY-400', 'id': 20400, 'name': 'Refunds that add up', 'color': 'color_4', 'done': False},
    {'key': 'PAY-380', 'id': 20380, 'name': 'Idempotent refunds API', 'color': 'color_8', 'done': False},
    {'key': 'PAY-360', 'id': 20360, 'name': 'Reconcile merchant reports with the ledger every night', 'color': 'color_3', 'done': False},
]
EPIC_OF = {'PAY-412': 'PAY-400', 'PAY-418': 'PAY-400', 'PAY-423': 'PAY-400', 'PAY-425': 'PAY-400', 'PAY-390': 'PAY-400',
           'PAY-421': 'PAY-380', 'PAY-409': 'PAY-380', 'PAY-401': 'PAY-380', 'PAY-402': 'PAY-380',
           'PAY-414': 'PAY-360', 'PAY-388': 'PAY-360'}

# key, summary, type, priority, assignee, state, labels, description, parent, comments
ISSUES = [
    ('PAY-418', 'Show the rounding rule in the refund preview', 'story', 'medium', '', 'todo', ['refunds'], 'Merchants see the refund before it goes; show how the amount was rounded under it.', None, 1),
    ('PAY-421', 'Retry refunds after a gateway timeout', 'task', 'high', 'kestrel', 'todo', ['refunds'], 'A refund that times out at the gateway is lost today. Retry it with the same idempotency key, three times.', None, 2),
    ('PAY-423', "Log the currency's minor unit with every refund", 'task', 'low', 'you', 'todo', ['refunds', 'logging'], 'So a rounding question can be answered from the log alone.', None, 0),
    ('PAY-425', 'Partial refunds on split tenders', 'story', 'medium', 'ember', 'todo', ['refunds'], 'A payment taken on two cards refunds to both, in the order they were charged.', None, 1),
    ('PAY-412', 'Refund rounds down on partial captures', 'bug', 'high', 'you', 'prog', ['refunds', 'rounding'], None, None, 3),
    ('PAY-409', 'Idempotency keys on POST /refunds', 'story', 'high', 'you', 'prog', ['refunds', 'api'], 'A retried POST with the same key returns the first answer instead of refunding twice.', None, 4),
    ('PAY-414', 'Read balances from the replica', 'task', 'medium', 'juniper', 'prog', ['ledger'], 'Balances and statements read from the replica; writes stay on the primary.', None, 0),
    ('PAY-398', 'Login keeps the last error', 'bug', 'medium', 'juniper', 'rev', ['auth'], 'After a failed sign-in, the next attempt still shows the old error until it returns.', None, 2),
    ('PAY-401', 'Rate limit per merchant', 'story', 'medium', 'ember', 'rev', ['api'], 'One token bucket per merchant, read from the plan.', None, 0),
    ('PAY-395', 'Split PaymentGateway into ports', 'story', 'medium', 'juniper', 'done', [], '', None, 0),
    ('PAY-402', 'Webhook signer v2 for every merchant', 'task', 'medium', 'ember', 'done', [], '', None, 0),
    ('PAY-390', 'Refund reasons in the export', 'story', 'low', 'you', 'done', [], '', None, 0),
    ('PAY-388', 'Currency codes from ISO 4217', 'task', 'low', 'kestrel', 'done', [], '', None, 0),
    ('PAY-386', 'Capture timeouts logged twice', 'bug', 'medium', 'kestrel', 'done', [], '', None, 0),
    ('PAY-413', 'Reproduce with a three-way split capture', 'sub', 'medium', 'you', 'done', ['rounding'], 'A capture of 10.00 in 3.34, 3.33 and 3.33, refunded whole: 9.99 back.', 'PAY-412', 0),
    ('PAY-415', 'Round half-even once, on the total', 'sub', 'high', 'you', 'prog', ['rounding'], 'Sum the parts first, then round once, half-even.', 'PAY-412', 0),
    ('PAY-416', 'Tests for ¥ and BHD minor units', 'sub', 'medium', '', 'todo', ['rounding'], 'Zero minor units (JPY) and three (BHD).', 'PAY-412', 0),
    ('PAY-410', 'Store the key with the refund', 'sub', 'medium', 'you', 'done', ['api'], "The key, the request's hash and the first answer, kept 24 hours.", 'PAY-409', 0),
    ('PAY-411', 'Answer a replay with the first response', 'sub', 'high', 'you', 'prog', ['api'], 'Same key and same body: the stored answer. Same key, another body: 422.', 'PAY-409', 0),
    ('PAY-417', 'Expire keys after 24 hours', 'sub', 'low', 'kestrel', 'todo', ['api'], 'A nightly job drops the keys older than a day.', 'PAY-409', 0),
]
BY = {i[0]: i for i in ISSUES}
IDS = {k: 30000 + int(k.split('-')[1]) for k in BY}

PAY_412 = """A partial capture refunds less than it should: {{RefundCalculator}} rounds each part down before summing, so a three-way split of 10.00 refunds 9.99.

Steps: capture 10.00 in three parts (3.34, 3.33, 3.33), then refund the whole capture.

Expected 10.00 back. Today: 9.99, and the ledger books a 0.01 remainder on the merchant.

!rounding-split.png|thumbnail!

Done when the refund equals what was captured for any split, rounding is half-even and once, on the total, currencies with 0 and 3 minor units have tests, and the merchant report shows the corrected amount."""

COMMENTS = {
    'PAY-412': [
        ('juniper', '2026-11-12T10:20:00.000+0000', 'Seen on three merchants since Tuesday; all three take split tenders.'),
        ('ember', '2026-11-13T06:05:00.000+0000', '[~accountid:u-you] the ledger job flags these as remainders. Can the fix also clear the ones already booked, or is that its own issue?'),
        ('you', '2026-11-13T07:40:00.000+0000', "Its own: this one stops new remainders. I'll write it up."),
    ],
}
UPDATED = {'PAY-412': '2026-11-13T07:40:00.000+0000', 'PAY-409': '2026-11-13T06:30:00.000+0000'}

def stamp(key):
    return UPDATED.get(key, '2026-11-%02dT09:00:00.000+0000' % (3 + int(key.split('-')[1]) % 9))

def comments_of(key, n):
    given = COMMENTS.get(key)
    if given is None:
        given = [(['juniper', 'kestrel', 'ember'][i % 3], '2026-11-1%dT09:%02d:00.000+0000' % (i % 3, i * 7),
                  'A note on %s, the %s.' % (key, ['first', 'second', 'third', 'fourth'][i])) for i in range(n)]
    out = []
    for ix, (who, at, body) in enumerate(given):
        out.append({'id': str(IDS[key] * 10 + ix), 'author': person(who), 'updateAuthor': person(who),
                    'body': body, 'created': at, 'updated': at})
    return out

def epic_of(key):
    e = next((e for e in EPICS if e['key'] == EPIC_OF.get(key)), None)
    if not e:
        return None
    return {'id': e['id'], 'key': e['key'], 'name': e['name'], 'summary': e['name'], 'color': {'key': e['color']}, 'done': e['done']}

def subtasks_of(key):
    return [brief(k) for k, *_ in ISSUES if BY[k][8] == key]

def brief(key):
    k, summary, kind, pri, who, state, *_ = BY[key]
    return {'id': str(IDS[k]), 'key': k, 'self': SITE + '/rest/api/2/issue/' + str(IDS[k]),
            'fields': {'summary': summary, 'status': STATES[state], 'priority': priority(pri), 'issuetype': TYPES[kind]}}

LINKS = {
    'PAY-412': [('Blocks', 'outward', 'PAY-425'), ('Relates', 'outward', 'PAY-426')],
    'PAY-425': [('Blocks', 'inward', 'PAY-412')],
}
LINK_TYPES = {'Blocks': ('is blocked by', 'blocks', '10000'), 'Relates': ('relates to', 'relates to', '10003'),
              'Duplicate': ('is duplicated by', 'duplicates', '10002'), 'Cloners': ('is cloned by', 'clones', '10001')}
EXTRA = {'PAY-426': ('PAY-426', 'Clear the remainders already booked', 'task', 'medium', '', 'todo', ['refunds'], '', None, 0)}

def links_of(key):
    out = []
    for ix, (kind, way, other) in enumerate(LINKS.get(key, [])):
        inward, outward, tid = LINK_TYPES[kind]
        src = BY.get(other) or EXTRA[other]
        BY.setdefault(other, src)
        IDS.setdefault(other, 30000 + int(other.split('-')[1]))
        link = {'id': str(40000 + IDS[key] % 1000 * 10 + ix),
                'type': {'id': tid, 'name': kind, 'inward': inward, 'outward': outward}}
        link['outwardIssue' if way == 'outward' else 'inwardIssue'] = brief(other)
        out.append(link)
    return out

ATTACH = {
    'PAY-412': [
        ('rounding-split.png', 48213, 'image/png', 'juniper', '2026-11-03T09:30:00.000+0000'),
        ('refund-log.txt', 4096, 'text/plain', 'juniper', '2026-11-03T09:31:00.000+0000'),
        ('ledger-remainders.png', 61840, 'image/png', 'ember', '2026-11-13T06:06:00.000+0000'),
    ],
}

def attachments_of(key):
    out = []
    for ix, (name, size, mime, who, at) in enumerate(ATTACH.get(key, [])):
        aid = str(50000 + IDS[key] % 1000 * 10 + ix)
        out.append({'id': aid, 'filename': name, 'size': size, 'mimeType': mime, 'author': person(who), 'created': at,
                    'content': SITE + '/rest/api/2/attachment/content/' + aid,
                    'thumbnail': SITE + '/rest/api/2/attachment/thumbnail/' + aid if mime.startswith('image/') else None})
    return out

def fields_of(key, whole):
    k, summary, kind, pri, who, state, labels, desc, parent, ncomments = BY[key]
    f = {'summary': summary, 'issuetype': TYPES[kind], 'status': STATES[state], 'priority': priority(pri),
         'assignee': person(who), 'reporter': person('juniper' if k == 'PAY-412' else (who or 'kestrel')),
         'labels': labels, 'updated': stamp(k), 'created': '2026-11-03T09:00:00.000+0000',
         'project': {'id': '10010', 'key': 'PAY', 'name': 'Payments'},
         'comment': {'comments': comments_of(k, ncomments) if whole else [], 'total': len(comments_of(k, ncomments)),
                     'maxResults': 50, 'startAt': 0},
         'subtasks': subtasks_of(k), 'issuelinks': links_of(k),
         'epic': epic_of(k), 'sprint': SPRINT,
         'customfield_10020': None if state == 'done' else [SPRINT],
         'customfield_10016': {'PAY-412': 3, 'PAY-409': 5, 'PAY-421': 2}.get(k),
         'duedate': '2026-11-14' if k == 'PAY-412' else None,
         'components': [{'id': '10100', 'name': 'Refunds API'}] if k in ('PAY-412', 'PAY-409', 'PAY-421') else [],
         'fixVersions': [{'id': '10200', 'name': '2.5', 'released': False}] if k in ('PAY-412', 'PAY-409') else []}
    if parent:
        f['parent'] = brief(parent)
    elif EPIC_OF.get(k):
        e = next(e for e in EPICS if e['key'] == EPIC_OF[k])
        f['parent'] = {'id': str(e['id']), 'key': e['key'],
                       'fields': {'summary': e['name'], 'status': STATES['prog'], 'issuetype': TYPES['epic']}}
    if whole:
        f['description'] = PAY_412 if k == 'PAY-412' else (desc or '')
        f['attachment'] = attachments_of(k)
    return f

def issue(key, whole):
    return {'id': str(IDS[key]), 'key': key, 'self': SITE + '/rest/agile/1.0/issue/' + str(IDS[key]),
            'fields': fields_of(key, whole)}

def write(folder, path, value):
    name = path.split('?')[0].replace('/', '_') + '.json'
    with open(os.path.join(folder, name), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)

def jira(folder):
    os.makedirs(folder)
    me = person('you')
    write(folder, '/rest/api/2/myself', dict(me, timeZone='Europe/London', locale='en_GB'))
    write(folder, '/rest/agile/1.0/board', {'isLast': True, 'values': [
        {'id': 7, 'name': 'Payments', 'type': 'scrum', 'location': {'projectKey': 'PAY', 'projectName': 'Payments'}},
        {'id': 8, 'name': 'Ledger', 'type': 'kanban', 'location': {'projectKey': 'LED', 'projectName': 'Ledger'}},
        {'id': 9, 'name': 'Platform', 'type': 'kanban', 'location': {'projectKey': 'PLAT', 'projectName': 'Platform'}},
    ]})
    write(folder, '/rest/agile/1.0/board/7/configuration', {'id': 7, 'name': 'Payments', 'columnConfig': {'columns': [
        {'name': 'To do', 'statuses': [{'id': '10000'}]},
        {'name': 'In progress', 'statuses': [{'id': '3'}]},
        {'name': 'In review', 'statuses': [{'id': '10001'}]},
        {'name': 'Done', 'statuses': [{'id': '10002'}]},
    ]}, 'location': {'type': 'project', 'key': 'PAY'}})
    write(folder, '/rest/agile/1.0/board/7/issue', {'startAt': 0, 'maxResults': 100, 'total': len(ISSUES),
                                                   'issues': [issue(k, False) for k, *_ in ISSUES]})
    write(folder, '/rest/agile/1.0/board/7/epic', {'isLast': True, 'values': EPICS})
    write(folder, '/rest/agile/1.0/board/7/sprint', {'isLast': True, 'values': [SPRINT, NEXT_SPRINT]})
    for b in (8, 9):
        write(folder, '/rest/agile/1.0/board/%d/configuration' % b, {'id': b, 'columnConfig': {'columns': [
            {'name': 'To do', 'statuses': [{'id': '10000'}]}, {'name': 'Done', 'statuses': [{'id': '10002'}]}]}})
        write(folder, '/rest/agile/1.0/board/%d/issue' % b, {'issues': []})
    for k, *_ in ISSUES:
        write(folder, '/rest/agile/1.0/issue/' + k, issue(k, True))
    write(folder, '/rest/agile/1.0/issue/PAY-426', issue('PAY-426', True))
    transitions = {
        'todo': [('21', 'Start', 'prog'), ('41', 'Done', 'done')],
        'prog': [('31', 'Review', 'rev'), ('41', 'Done', 'done'), ('11', 'Stop', 'todo')],
        'rev': [('51', 'Back to work', 'prog'), ('41', 'Done', 'done')],
        'done': [('61', 'Reopen', 'todo')],
    }
    resolution = {'resolution': {'required': True, 'name': 'Resolution', 'schema': {'type': 'resolution', 'system': 'resolution'},
                                 'allowedValues': [{'id': '1', 'name': 'Done'}, {'id': '2', 'name': "Won't do"}, {'id': '3', 'name': 'Duplicate'}]}}
    for k, *_ in ISSUES:
        state = BY[k][5]
        write(folder, '/rest/api/2/issue/%s/transitions' % k, {'transitions': [
            {'id': tid, 'name': name, 'to': STATES[to], 'hasScreen': to == 'done', 'fields': resolution if to == 'done' else {}}
            for tid, name, to in transitions[state]]})
    edit = {'fields': {
        'summary': {'required': True, 'name': 'Summary', 'schema': {'type': 'string', 'system': 'summary'}, 'operations': ['set']},
        'description': {'required': False, 'name': 'Description', 'schema': {'type': 'string', 'system': 'description'}, 'operations': ['set']},
        'priority': {'required': False, 'name': 'Priority', 'schema': {'type': 'priority', 'system': 'priority'}, 'operations': ['set'],
                     'allowedValues': [priority(p) for p in ('highest', 'high', 'medium', 'low', 'lowest')]},
        'labels': {'required': False, 'name': 'Labels', 'schema': {'type': 'array', 'items': 'string', 'system': 'labels'}, 'operations': ['add', 'set', 'remove']},
        'duedate': {'required': False, 'name': 'Due date', 'schema': {'type': 'date', 'system': 'duedate'}, 'operations': ['set']},
        'components': {'required': False, 'name': 'Components', 'schema': {'type': 'array', 'items': 'component', 'system': 'components'}, 'operations': ['add', 'set', 'remove'],
                       'allowedValues': [{'id': '10100', 'name': 'Refunds API'}, {'id': '10101', 'name': 'Ledger'}, {'id': '10102', 'name': 'Merchant reports'}]},
        'customfield_10016': {'required': False, 'name': 'Story points', 'schema': {'type': 'number', 'custom': 'com.atlassian.jira.plugin.system.customfieldtypes:float', 'customId': 10016}, 'operations': ['set']},
        'fixVersions': {'required': False, 'name': 'Fix versions', 'schema': {'type': 'array', 'items': 'version', 'system': 'fixVersions'}, 'operations': ['add', 'set', 'remove'],
                        'allowedValues': [{'id': '10200', 'name': '2.5', 'released': False}, {'id': '10201', 'name': '2.6', 'released': False}]},
        'customfield_10020': {'required': False, 'name': 'Sprint', 'schema': {'type': 'array', 'items': 'json', 'custom': 'com.pyxis.greenhopper.jira:gh-sprint', 'customId': 10020}, 'operations': ['set']},
        'assignee': {'required': False, 'name': 'Assignee', 'schema': {'type': 'user', 'system': 'assignee'}, 'operations': ['set']},
    }}
    for k, *_ in ISSUES:
        write(folder, '/rest/api/2/issue/%s/editmeta' % k, edit)
    write(folder, '/rest/api/2/user/assignable/search', [person(p) for p in ('you', 'juniper', 'kestrel', 'ember')])
    write(folder, '/rest/api/2/priority', [priority(p) for p in ('highest', 'high', 'medium', 'low', 'lowest')])
    write(folder, '/rest/api/2/issueLinkType', {'issueLinkTypes': [
        {'id': tid, 'name': n, 'inward': i, 'outward': o} for n, (i, o, tid) in LINK_TYPES.items()]})
    write(folder, '/rest/api/2/project', [
        {'id': '10010', 'key': 'PAY', 'name': 'Payments'}, {'id': '10011', 'key': 'LED', 'name': 'Ledger'},
        {'id': '10012', 'key': 'PLAT', 'name': 'Platform'}])
    write(folder, '/rest/api/2/issue/createmeta/PAY/issuetypes', {'values': [
        dict(TYPES[t], id=TYPES[t]['id']) for t in ('task', 'bug', 'story', 'sub')]})
    create = [
        {'fieldId': 'summary', 'required': True, 'name': 'Summary', 'schema': {'type': 'string', 'system': 'summary'}},
        {'fieldId': 'description', 'required': False, 'name': 'Description', 'schema': {'type': 'string', 'system': 'description'}},
        {'fieldId': 'assignee', 'required': False, 'name': 'Assignee', 'schema': {'type': 'user', 'system': 'assignee'}},
        {'fieldId': 'priority', 'required': False, 'name': 'Priority', 'schema': {'type': 'priority', 'system': 'priority'},
         'allowedValues': [priority(p) for p in ('highest', 'high', 'medium', 'low', 'lowest')], 'hasDefaultValue': True, 'defaultValue': priority('medium')},
        {'fieldId': 'labels', 'required': False, 'name': 'Labels', 'schema': {'type': 'array', 'items': 'string', 'system': 'labels'}},
        {'fieldId': 'customfield_10020', 'required': False, 'name': 'Sprint', 'schema': {'type': 'array', 'items': 'json', 'custom': 'com.pyxis.greenhopper.jira:gh-sprint'}},
        {'fieldId': 'components', 'required': True, 'name': 'Components', 'schema': {'type': 'array', 'items': 'component', 'system': 'components'},
         'allowedValues': [{'id': '10100', 'name': 'Refunds API'}, {'id': '10101', 'name': 'Ledger'}, {'id': '10102', 'name': 'Merchant reports'}]},
    ]
    for t in ('task', 'bug', 'story', 'sub'):
        fields = create + ([{'fieldId': 'parent', 'required': True, 'name': 'Parent', 'schema': {'type': 'issuelink', 'system': 'parent'}}] if t == 'sub' else [])
        write(folder, '/rest/api/2/issue/createmeta/PAY/issuetypes/' + TYPES[t]['id'], {'values': fields})
    # News: what changed lately on the issues you are on, with their changes (the plugin's `news` read).
    news = []
    for k, change in (('PAY-412', ('assignee', 'juniper', 'you', 'juniper', '2026-11-13T07:30:00.000+0000')),
                      ('PAY-390', ('status', 'In Review', 'Done', 'kestrel', '2026-11-12T15:00:00.000+0000')),
                      ('PAY-423', ('comment', '', '', 'juniper', '2026-11-11T11:00:00.000+0000'))):
        i = issue(k, True)
        field, frm, to, who, at = change
        if field == 'assignee':
            items = [{'field': 'assignee', 'fieldId': 'assignee', 'from': PEOPLE[frm][0], 'fromString': PEOPLE[frm][1],
                      'to': PEOPLE[to][0], 'toString': PEOPLE[to][1]}]
            i['changelog'] = {'histories': [{'id': '1', 'author': person(who), 'created': at, 'items': items}]}
        elif field == 'status':
            items = [{'field': 'status', 'fieldId': 'status', 'fromString': frm, 'toString': to}]
            i['changelog'] = {'histories': [{'id': '2', 'author': person(who), 'created': at, 'items': items}]}
            i['fields']['status'] = STATES['done']
        else:
            i['fields']['comment']['comments'] = [{'id': '9001', 'author': person(who), 'body': 'Use the ISO table we already ship.', 'created': at, 'updated': at}]
            i['changelog'] = {'histories': []}
        news.append(i)
    write(folder, '/rest/api/2/search@news', {'issues': news})
    write(folder, '/rest/api/2/search', {'issues': [issue(k, False) for k in ('PAY-412', 'PAY-415', 'PAY-416', 'PAY-418')]})
    write(folder, 'POST/rest/api/2/issue', {'id': '30427', 'key': 'PAY-427', 'self': SITE + '/rest/api/2/issue/30427'})
    # A move PAY-412's validator refuses: Done while a sub-task is open.
    write(folder, 'POST/rest/api/2/issue/PAY-412/transitions', {'answers': [
        {'when': '"41"', 'status': 400, 'errorMessages': ["PAY-415 is still open"]}]})

def png(path, w, h, bars):
    """A picture of `bars` ((x, y, w, h, rgb), …) on a dark ground, as a PNG: what an attachment's bytes stand for."""
    rows = []
    for y in range(h):
        row = bytearray([0])
        for x in range(w):
            rgb = (34, 38, 46)
            for bx, by, bw, bh, c in bars:
                if bx <= x < bx + bw and by <= y < by + bh:
                    rgb = c
            row += bytes(rgb)
        rows.append(bytes(row))
    raw = zlib.compress(b''.join(rows), 9)
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)
    with open(path, 'wb') as f:
        f.write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)) +
                chunk(b'IDAT', raw) + chunk(b'IEND', b''))

def files(folder):
    os.makedirs(folder)
    blue, green, amber = (97, 175, 239), (152, 195, 121), (229, 192, 123)
    for name, aid in (('rounding-split.png', '54120'), ('ledger-remainders.png', '54122')):
        bars = [(40, 160 - 40 * i, 60, 40 * i + 20, [blue, green, amber][i % 3]) for i in range(1, 4)] if 'split' in name else \
               [(30 + 50 * i, 60, 36, 120, [amber, blue][i % 2]) for i in range(5)]
        png(os.path.join(folder, aid), 320, 200, bars)
    with open(os.path.join(folder, '54121'), 'w') as f:
        f.write('refund 10.00 in 3 parts: 3.34 3.33 3.33\nrefunded 9.99\n')

# Trello: a board of the same people, read as Trello's REST API answers (api.trello.com/1/…).
TRELLO = 'trello.com'
MEMBERS = {'you': 'aquinn', 'juniper': 'jhale', 'kestrel': 'kokafor', 'ember': 'elind'}
def tid(at, n):
    """A Trello id: its first four bytes the time it was made."""
    import calendar, time
    secs = calendar.timegm(time.strptime(at[:19], '%Y-%m-%dT%H:%M:%S'))
    return '%08x%016x' % (secs, n)
def member(who):
    return {'id': tid('2025-01-01T00:00:00', 900 + list(MEMBERS).index(who)), 'username': MEMBERS[who],
            'fullName': PEOPLE[who][1]}
BOARD = {'id': tid('2026-09-01T09:00:00', 1), 'name': 'Website relaunch', 'shortLink': 'kZ3fG9aB'}
LISTS = [{'id': tid('2026-09-01T09:00:00', 10 + i), 'name': n, 'pos': 1024 * (i + 1)}
         for i, n in enumerate(['Next up', 'Doing', 'Review', 'Done'])]
LIST = {l['name']: l for l in LISTS}
LABELS = {'design': {'id': 'lb1', 'name': 'design', 'color': 'blue'}, 'seo': {'id': 'lb2', 'name': 'seo', 'color': 'green'},
          'copy': {'id': 'lb3', 'name': 'copy', 'color': 'yellow'}, '': {'id': 'lb4', 'name': '', 'color': 'red'}}
# short link, name, list, members, labels, made, last touched, due, description
CARDS = [
    ('hP4sK7vL', 'Pricing page in the new layout', 'Doing', ['you'], ['design'], '2026-11-02T10:00:00', '2026-11-13T08:05:00', '2026-11-18',
     'The pricing page moves to the new grid.\n\n- Three plans side by side, the middle one marked\n- Yearly and monthly prices under one toggle\n- The FAQ below, folded\n\nThe **Pricing** frame in the relaunch file has the layout.'),
    ('mR2cX9qD', 'Move the blog to the static build', 'Doing', ['juniper'], [], '2026-11-03T11:00:00', '2026-11-12T15:20:00', None,
     'Posts as markdown in the repository, built with the site.'),
    ('tY6bN3wE', 'Redirects for the old URLs', 'Next up', [], ['seo'], '2026-11-04T09:30:00', '2026-11-11T10:00:00', None,
     'Every old address answers 301 to its new page; the list is in the sheet.'),
    ('vB5nM4kS', 'Hero image for the home page', 'Next up', ['kestrel'], ['design'], '2026-11-05T14:00:00', '2026-11-12T09:40:00', '2026-11-20',
     'Under 200 KB on mobile, in two crops.'),
    ('gW7eR2tY', 'Lighthouse score over 90 on mobile', 'Review', ['you'], [''], '2026-11-06T08:00:00', '2026-11-13T07:55:00', None,
     'Measured on the staging build, a mid-range phone profile.'),
    ('qF8jZ1uA', 'Cookie banner copy', 'Review', ['ember'], ['copy'], '2026-11-06T16:00:00', '2026-11-12T17:10:00', None,
     'Short, one button to accept and one to choose.'),
    ('cJ3hL8pX', 'Contact form sends to the CRM', 'Done', ['kestrel'], [], '2026-10-28T10:00:00', '2026-11-07T12:00:00', None, ''),
    ('dK9mV6bQ', 'Footer links checked', 'Done', ['juniper'], [], '2026-10-30T10:00:00', '2026-11-05T12:00:00', None, ''),
]
CARD_COMMENTS = {
    'hP4sK7vL': [('juniper', '2026-11-12T16:20:00', 'The toggle for yearly prices could sit above the table. @aquinn what do you think?'),
                 ('ember', '2026-11-13T08:05:00', 'The plans\' copy is in the doc, ready for the page.')],
    'gW7eR2tY': [('ember', '2026-11-13T07:55:00', 'Mobile is at 84; the hero image is most of what is left.')],
}
CARD_ATTACH = {'hP4sK7vL': [('pricing-sketch.png', 'image/png', 21480, '2026-11-12T16:18:00')]}

def card(c, whole):
    short, name, lst, who, labels, made, touched, due, desc = c
    out = {'id': tid(made, 100 + CARDS.index(c)), 'shortLink': short, 'shortUrl': 'https://trello.com/c/' + short,
           'name': name, 'desc': desc, 'idList': LIST[lst]['id'], 'idBoard': BOARD['id'],
           'idMembers': [member(w)['id'] for w in who], 'labels': [LABELS[l] for l in labels],
           'dateLastActivity': touched + '.000Z', 'due': due + 'T17:00:00.000Z' if due else None,
           'badges': {'comments': len(CARD_COMMENTS.get(short, [])), 'attachments': len(CARD_ATTACH.get(short, []))}}
    if whole:
        out['list'] = {'id': LIST[lst]['id'], 'name': lst}
        out['board'] = {'id': BOARD['id'], 'name': BOARD['name'], 'shortLink': BOARD['shortLink']}
        out['members'] = [member(w) for w in who]
        actions = [{'id': tid(at, 500 + i), 'type': 'commentCard', 'date': at + '.000Z',
                    'memberCreator': member(w), 'data': {'text': text}}
                   for i, (w, at, text) in enumerate(CARD_COMMENTS.get(short, []))]
        actions.append({'id': tid(made, 400), 'type': 'createCard', 'date': made + '.000Z',
                        'memberCreator': member('juniper'), 'data': {}})
        out['actions'] = sorted(actions, key=lambda a: a['date'], reverse=True)
        out['attachments'] = [{'id': tid(at, 600 + i), 'name': n, 'mimeType': mime, 'bytes': size, 'date': at + '.000Z',
                               'url': 'https://trello.com/1/cards/%s/attachments/%s/download/%s' % (out['id'], tid(at, 600 + i), n)}
                              for i, (n, mime, size, at) in enumerate(CARD_ATTACH.get(short, []))]
    return out

def trello(folder):
    os.makedirs(folder)
    write(folder, '/1/members/me', member('you'))
    for who in MEMBERS:
        write(folder, '/1/members/' + MEMBERS[who], {'id': member(who)['id']})
    write(folder, '/1/members/me/boards', [BOARD])
    write(folder, '/1/boards/' + BOARD['shortLink'], dict(BOARD, lists=LISTS, cards=[card(c, False) for c in CARDS],
                                                          members=[member(w) for w in MEMBERS]))
    write(folder, '/1/boards/%s/members' % BOARD['shortLink'], [member(w) for w in MEMBERS])
    for c in CARDS:
        write(folder, '/1/cards/' + c[0], card(c, True))
    by = {c[0]: c for c in CARDS}
    def told(n, kind, who, short, at, **data):
        return {'id': tid(at, 700 + n), 'type': kind, 'date': at + '.000Z', 'unread': True, 'memberCreator': member(who),
                'data': dict(data, card={'id': card(by[short], False)['id'], 'name': by[short][1], 'shortLink': short},
                             board={'id': BOARD['id'], 'name': BOARD['name'], 'shortLink': BOARD['shortLink']})}
    write(folder, '/1/members/me/notifications@news', [
        told(1, 'commentCard', 'ember', 'gW7eR2tY', '2026-11-13T07:55:00', text=CARD_COMMENTS['gW7eR2tY'][0][2]),
        told(2, 'addedToCard', 'juniper', 'gW7eR2tY', '2026-11-13T07:30:00'),
        told(3, 'mentionedOnCard', 'juniper', 'hP4sK7vL', '2026-11-12T16:20:00', text=CARD_COMMENTS['hP4sK7vL'][0][2]),
        told(4, 'changeCard', 'kestrel', 'hP4sK7vL', '2026-11-12T11:00:00', listBefore=LIST['Next up'], listAfter=LIST['Doing']),
    ])
    write(folder, '/1/search', {'cards': [card(by[k], True) for k in ('hP4sK7vL', 'vB5nM4kS')]})
    write(folder, 'POST/1/cards', {'id': tid('2026-11-13T09:00:00', 199), 'shortLink': 'nW8pQ2rT'})
    os.makedirs(os.path.join(folder, 'files'))
    png(os.path.join(folder, 'files', 'pricing-sketch.png'), 320, 200,
        [(30 + 90 * i, 50, 70, 120, [(97, 175, 239), (229, 192, 123), (152, 195, 121)][i]) for i in range(3)])

def main():
    if os.path.isdir(OUT):
        shutil.rmtree(OUT)
    os.makedirs(OUT)
    with open(os.path.join(OUT, 'trackers.txt'), 'w') as f:
        f.write('# The fixture\'s sites, as the plugin\'s trackers.txt names them.\n%s you@acme.example\n' % SITE)
        f.write('https://%s fixture-api-key\n' % TRELLO)
    jira(os.path.join(OUT, JIRA))
    files(os.path.join(OUT, JIRA, 'files'))
    trello(os.path.join(OUT, TRELLO))

main()
