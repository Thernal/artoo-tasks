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
    'backlog': {'id': '10004', 'name': 'Backlog', 'statusCategory': {'id': 2, 'key': 'new', 'name': 'To Do'}},
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
        ('ember', '2026-11-13T06:05:00.000+0000', '[~accountid:u-you] the ledger job flags these as remainders. Can the fix also clear the ones already booked, or is that its own issue?\n\n!ledger-remainders.png|thumbnail!'),
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
    return [brief(k) for k, i in BY.items() if i[8] == key]

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
EXTRA = {'PAY-426': ('PAY-426', 'Clear the remainders already booked', 'task', 'medium', '', 'todo', ['refunds'], '', None, 0),
         # What New task makes (POST /rest/api/2/issue answers its key): in the workflow's first state, as Jira makes one.
         'PAY-427': ('PAY-427', 'Rounding note in the refund sheet', 'task', 'medium', 'you', 'todo', ['refunds'],
                     'Under the amount, one line: how it was rounded and why.', None, 0)}

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
         'project': project_of(k),
         'comment': {'comments': comments_of(k, ncomments) if whole else [], 'total': len(comments_of(k, ncomments)),
                     'maxResults': 50, 'startAt': 0},
         'subtasks': subtasks_of(k), 'issuelinks': links_of(k),
         'epic': epic_of(k), 'sprint': SPRINT_OF[k][0] if k in SPRINT_OF else SPRINT,
         'customfield_10020': SPRINT_OF[k][1] if k in SPRINT_OF else (None if state == 'done' else [SPRINT]),
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

PROJECTS = {'PAY': ('10010', 'Payments', 30000), 'LED': ('10011', 'Ledger', 40000), 'PLAT': ('10012', 'Platform', 50000),
            'WAL': ('10013', 'Wallet', 70000), 'CHK': ('10014', 'Checkout', 80000)}

def project_of(key):
    p = key.split('-')[0]
    return {'id': PROJECTS[p][0], 'key': p, 'name': PROJECTS[p][1]}

# A board's slices. Payments (7, scrum) has two active sprints (Sprint 34 holds the issues above), a future sprint with
# nothing planned, five closed sprints and a Backlog; Ledger (8) and Platform (9) are kanban, Ledger with a Backlog
# column; Wallet (10) is team-managed with sprints off; Checkout (11) is scrum with no sprint running.
SECOND_SPRINT = {'id': 36, 'name': 'Reconciliation squad Q4 sprint 3', 'state': 'active',
                 'startDate': '2026-11-10T09:00:00.000Z', 'endDate': '2026-11-21T17:00:00.000Z', 'originBoardId': 7,
                 'goal': 'Move every nightly reconciliation job onto the replica, retire the old ledger exporter and get '
                         'the merchant statement totals to match the bank to the cent before the quarter closes'}
CLOSED_SPRINTS = [{'id': n, 'name': 'Sprint %d' % n, 'state': 'closed', 'originBoardId': 7, 'goal': '',
                   'startDate': '2026-%02d-%02dT09:00:00.000Z' % start, 'endDate': '2026-%02d-%02dT17:00:00.000Z' % end,
                   'completeDate': '2026-%02d-%02dT16:30:00.000Z' % end}
                  for n, start, end in ((29, (8, 25), (9, 5)), (30, (9, 8), (9, 19)), (31, (9, 22), (10, 3)),
                                        (32, (10, 6), (10, 17)), (33, (10, 20), (10, 31)))]
CHECKOUT_SPRINT = {'id': 40, 'name': 'Checkout sprint 1', 'state': 'future', 'originBoardId': 11,
                   'startDate': '2026-12-01T09:00:00.000Z', 'endDate': '2026-12-12T17:00:00.000Z', 'goal': ''}
# An issue's sprint field and its Sprint field's list, for those not in Sprint 34.
SPRINT_OF = {}
VERBS = ['Retry', 'Log', 'Show', 'Cache', 'Explain', 'Rename', 'Split', 'Validate', 'Export', 'Batch', 'Throttle',
         'Audit', 'Archive', 'Backfill', 'Index', 'Document']
OBJECTS = ['refunds after a gateway timeout', 'the settlement report', 'merchant payouts', 'chargeback notices',
           'the currency table', 'webhook deliveries', 'failed captures', 'the nightly ledger close', 'statement totals',
           'card tokens', 'dispute evidence', 'payout holds', 'fee schedules', 'the reconciliation diff',
           'partial refunds', 'merchant onboarding checks', 'the risk score', 'invoice numbers', 'tax lines on receipts',
           'the payout calendar']
PEOPLE_CYCLE = ['kestrel', 'juniper', '', 'ember', 'juniper', 'kestrel', 'ember', '', 'kestrel']
NEXT = {}

def generated(prefix, n, states, sprint=None, sprints=None, epics=()):
    """`n` invented issues of project `prefix`, each eighth with two sub-tasks after it: their keys, in rank order."""
    keys = []
    def add(kind, summary, who, state, parent, i):
        num = NEXT.setdefault(prefix, 1001)
        NEXT[prefix] = num + 1
        k = '%s-%d' % (prefix, num)
        BY[k] = (k, summary, kind, ['medium', 'high', 'low', 'medium', 'highest'][i % 5], who, state, [], '', parent, i % 3)
        IDS[k] = PROJECTS[prefix][2] + num
        SPRINT_OF[k] = (sprint, sprints)
        if epics and not parent and epics[i % len(epics)]:
            EPIC_OF[k] = epics[i % len(epics)]
        keys.append(k)
        return k
    for i in range(n):
        g = NEXT.get(prefix, 1001)
        who = 'you' if g % 9 == 4 else PEOPLE_CYCLE[g % len(PEOPLE_CYCLE)]
        summary = '%s %s' % (VERBS[g % len(VERBS)], OBJECTS[(g * 7 + g // len(VERBS)) % len(OBJECTS)])
        parent = add(['task', 'story', 'bug', 'task', 'story'][g % 5], summary, who, states[g % len(states)], None, g)
        if i % 8 == 3:
            for j, what in enumerate(('Write the tests for it', 'Roll it out behind a flag')):
                add('sub', what, who, states[(g + j) % len(states)], parent, g + j)
    return keys

def listed(keys):
    return {'startAt': 0, 'maxResults': len(keys), 'total': len(keys), 'issues': [issue(k, False) for k in keys]}

def slice_files(folder, path, keys, asks=('mine',)):
    """A slice's issues at `path`, and the answers to its JQL reads: yours, the words `refund`, epic PAY-400's."""
    write(folder, path, listed(keys))
    picks = {'mine': lambda k: BY[k][4] == 'you', 'words': lambda k: 'refund' in BY[k][1].lower(),
             'epic': lambda k: EPIC_OF.get(k) == 'PAY-400'}
    for ask in asks:
        write(folder, path + '@' + ask, listed([k for k in keys if picks[ask](k)]))

def columns(*names):
    ids = {'Backlog': '10004', 'To do': '10000', 'Selected for development': '10000', 'In progress': '3', 'Done': '10002'}
    return {'columns': [{'name': n, 'statuses': [{'id': ids[n]}]} for n in names]}

def slices(folder):
    work = ['todo', 'todo', 'prog', 'todo', 'rev', 'done', 'prog', 'done']
    epics = ('PAY-400', 'PAY-380', 'PAY-360', None)
    first = [k for k, *_ in ISSUES] + generated('PAY', 64, work, SPRINT, [SPRINT], epics)
    second = generated('PAY', 136, work, SECOND_SPRINT, [SECOND_SPRINT], epics)
    closed = {s['id']: generated('PAY', 10, ['done'], None, [s], epics) for s in CLOSED_SPRINTS}
    backlog = generated('PAY', 300, ['todo'], None, None, epics)
    write(folder, '/rest/agile/1.0/board/7/sprint', {'isLast': True, 'values': [SPRINT, NEXT_SPRINT, SECOND_SPRINT]})
    write(folder, '/rest/agile/1.0/board/7/sprint@closed', {'isLast': True, 'values': CLOSED_SPRINTS})
    slice_files(folder, '/rest/agile/1.0/sprint/34/issue', first)
    slice_files(folder, '/rest/agile/1.0/sprint/36/issue', second)
    slice_files(folder, '/rest/agile/1.0/sprint/35/issue', [])
    for sid, keys in closed.items():
        slice_files(folder, '/rest/agile/1.0/sprint/%d/issue' % sid, keys)
    slice_files(folder, '/rest/agile/1.0/board/7/backlog', backlog, ('mine', 'words', 'epic'))
    # An epic's issues wherever they are: its whole count, beside a slice's part of it.
    everywhere = first + second + backlog + [k for keys in closed.values() for k in keys]
    for e in EPICS:
        write(folder, '/rest/agile/1.0/epic/%s/issue' % e['key'],
              listed([k for k in everywhere if EPIC_OF.get(k) == e['key']]))

    ledger = generated('LED', 40, ['backlog', 'backlog', 'todo', 'prog', 'backlog', 'done', 'todo', 'backlog'])
    write(folder, '/rest/agile/1.0/board/8/configuration', {'id': 8, 'name': 'Ledger', 'type': 'kanban',
          'columnConfig': columns('Backlog', 'Selected for development', 'In progress', 'Done'),
          'location': {'type': 'project', 'key': 'LED'}})
    slice_files(folder, '/rest/agile/1.0/board/8/issue', ledger)
    write(folder, '/rest/agile/1.0/board/8/backlog', listed([k for k in ledger if BY[k][5] == 'backlog']))
    platform = generated('PLAT', 30, ['todo', 'prog', 'done', 'done', 'todo'])
    write(folder, '/rest/agile/1.0/board/9/configuration', {'id': 9, 'name': 'Platform', 'type': 'kanban',
          'columnConfig': columns('To do', 'In progress', 'Done'), 'location': {'type': 'project', 'key': 'PLAT'}})
    slice_files(folder, '/rest/agile/1.0/board/9/issue', platform)
    wallet = generated('WAL', 20, ['todo', 'prog', 'todo', 'done'])
    write(folder, '/rest/agile/1.0/board/10/configuration', {'id': 10, 'name': 'Wallet', 'type': 'simple',
          'columnConfig': columns('To do', 'In progress', 'Done'), 'location': {'type': 'project', 'key': 'WAL'}})
    slice_files(folder, '/rest/agile/1.0/board/10/issue', wallet)
    write(folder, '/rest/agile/1.0/board/10/sprint', {'status': 400, 'errorMessages': ['The board does not support sprints']})
    planned = generated('CHK', 8, ['todo'], CHECKOUT_SPRINT, [CHECKOUT_SPRINT])
    waiting = generated('CHK', 25, ['todo'])
    write(folder, '/rest/agile/1.0/board/11/configuration', {'id': 11, 'name': 'Checkout', 'type': 'scrum',
          'columnConfig': columns('To do', 'In progress', 'Done'), 'location': {'type': 'project', 'key': 'CHK'}})
    write(folder, '/rest/agile/1.0/board/11/issue', listed(planned + waiting))
    write(folder, '/rest/agile/1.0/board/11/sprint', {'isLast': True, 'values': [CHECKOUT_SPRINT]})
    slice_files(folder, '/rest/agile/1.0/sprint/40/issue', planned)
    slice_files(folder, '/rest/agile/1.0/board/11/backlog', waiting, ('mine', 'words'))
    for keys in [first[len(ISSUES):], second, backlog, ledger, platform, wallet, planned, waiting, *closed.values()]:
        for k in keys:
            write(folder, '/rest/agile/1.0/issue/' + k, issue(k, True))

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
        {'id': 10, 'name': 'Wallet', 'type': 'simple', 'location': {'projectKey': 'WAL', 'projectName': 'Wallet'}},
        {'id': 11, 'name': 'Checkout', 'type': 'scrum', 'location': {'projectKey': 'CHK', 'projectName': 'Checkout'}},
    ]})
    write(folder, '/rest/agile/1.0/board/7/configuration', {'id': 7, 'name': 'Payments', 'type': 'scrum', 'columnConfig': {'columns': [
        {'name': 'To do', 'statuses': [{'id': '10000'}]},
        {'name': 'In progress', 'statuses': [{'id': '3'}]},
        {'name': 'In review', 'statuses': [{'id': '10001'}]},
        {'name': 'Done', 'statuses': [{'id': '10002'}]},
    ]}, 'location': {'type': 'project', 'key': 'PAY'}})
    write(folder, '/rest/agile/1.0/board/7/issue', {'startAt': 0, 'maxResults': 100, 'total': len(ISSUES),
                                                   'issues': [issue(k, False) for k, *_ in ISSUES]})
    write(folder, '/rest/agile/1.0/board/7/epic', {'isLast': True, 'values': EPICS})
    slices(folder)
    for k, *_ in ISSUES:
        write(folder, '/rest/agile/1.0/issue/' + k, issue(k, True))
    write(folder, '/rest/agile/1.0/issue/PAY-426', issue('PAY-426', True))
    BY.setdefault('PAY-427', EXTRA['PAY-427'])
    IDS.setdefault('PAY-427', 30427)
    write(folder, '/rest/agile/1.0/issue/PAY-427', issue('PAY-427', True))
    transitions = {
        'todo': [('21', 'Start', 'prog'), ('41', 'Done', 'done')],
        'prog': [('31', 'Review', 'rev'), ('41', 'Done', 'done'), ('11', 'Stop', 'todo')],
        'rev': [('51', 'Back to work', 'prog'), ('41', 'Done', 'done')],
        'done': [('61', 'Reopen', 'todo')],
    }
    resolution = {'resolution': {'required': True, 'name': 'Resolution', 'schema': {'type': 'resolution', 'system': 'resolution'},
                                 'allowedValues': [{'id': '1', 'name': 'Done'}, {'id': '2', 'name': "Won't do"}, {'id': '3', 'name': 'Duplicate'}]}}
    for k in [k for k, *_ in ISSUES] + ['PAY-427']:
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
    for k in [k for k, *_ in ISSUES] + ['PAY-427']:
        write(folder, '/rest/api/2/issue/%s/editmeta' % k, edit)
    write(folder, '/rest/api/2/user/assignable/search', [person(p) for p in ('you', 'juniper', 'kestrel', 'ember')])
    write(folder, '/rest/api/2/priority', [priority(p) for p in ('highest', 'high', 'medium', 'low', 'lowest')])
    write(folder, '/rest/api/2/issueLinkType', {'issueLinkTypes': [
        {'id': tid, 'name': n, 'inward': i, 'outward': o} for n, (i, o, tid) in LINK_TYPES.items()]})
    write(folder, '/rest/api/2/project', [
        {'id': '10010', 'key': 'PAY', 'name': 'Payments'}, {'id': '10011', 'key': 'LED', 'name': 'Ledger'},
        {'id': '10012', 'key': 'PLAT', 'name': 'Platform'}, {'id': '10013', 'key': 'WAL', 'name': 'Wallet'},
        {'id': '10014', 'key': 'CHK', 'name': 'Checkout'}])
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
    # The site is Jira Cloud's, which searches at /search/jql.
    write(folder, '/rest/api/2/search/jql@news', {'issues': news})
    write(folder, '/rest/api/2/search/jql', {'issues': [issue(k, False) for k in ('PAY-412', 'PAY-415', 'PAY-416', 'PAY-418')]})
    write(folder, 'POST/rest/api/2/issue', {'id': '30427', 'key': 'PAY-427', 'self': SITE + '/rest/api/2/issue/30427'})
    # A move PAY-412's validator refuses: Done while a sub-task is open.
    write(folder, 'POST/rest/api/2/issue/PAY-412/transitions', {'answers': [
        {'when': '"41"', 'status': 400, 'errorMessages': ["PAY-415 is still open"]}]})
    # A move into sprint 35 Jira refuses for PAY-418 alone: its row stays where it was, these words on it.
    write(folder, 'POST/rest/agile/1.0/sprint/35/issue', {'answers': [
        {'when': '"PAY-418"', 'status': 400,
         'errorMessages': ["PAY-418 can't join a sprint that has not started while it is ranked in another"]}]})

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

SECOND = 'ops@acme.example'
SECOND_KEY = JIRA + '~ops'
OPS_ISSUES = [('OPS-7', 'Rotate the gateway certificates', 'task', 'high', 'todo'),
              ('OPS-9', 'Alert when a payout batch runs late', 'story', 'medium', 'prog')]

def second(folder):
    """A second account on the Jira site, `ops@acme.example`: someone else's sign-in, with a board of its own."""
    os.makedirs(folder)
    rowan = {'accountId': 'u-rowan', 'displayName': 'Rowan Vale', 'active': True, 'avatarUrls': {'48x48': ''}, 'emailAddress': ''}
    write(folder, '/rest/api/2/myself', dict(rowan, timeZone='Europe/London', locale='en_GB'))
    write(folder, '/rest/agile/1.0/board', {'isLast': True, 'values': [
        {'id': 21, 'name': 'Operations', 'type': 'kanban', 'location': {'projectKey': 'OPS', 'projectName': 'Operations'}}]})
    write(folder, '/rest/agile/1.0/board/21/configuration', {'id': 21, 'name': 'Operations', 'columnConfig': {'columns': [
        {'name': 'To do', 'statuses': [{'id': '10000'}]}, {'name': 'In progress', 'statuses': [{'id': '3'}]},
        {'name': 'Done', 'statuses': [{'id': '10002'}]}]}, 'location': {'type': 'project', 'key': 'OPS'}})
    def one(n, key, summary, kind, pri, state):
        return {'id': str(60000 + n), 'key': key, 'self': SITE + '/rest/agile/1.0/issue/' + str(60000 + n),
                'fields': {'summary': summary, 'issuetype': TYPES[kind], 'status': STATES[state], 'priority': priority(pri),
                           'assignee': rowan, 'reporter': rowan, 'labels': [], 'updated': '2026-11-12T10:00:00.000+0000',
                           'created': '2026-11-10T09:00:00.000+0000', 'project': {'id': '10020', 'key': 'OPS', 'name': 'Operations'},
                           'comment': {'comments': [], 'total': 0, 'maxResults': 50, 'startAt': 0}, 'subtasks': [], 'issuelinks': [],
                           'description': '', 'attachment': []}}
    issues = [one(n, *i) for n, i in enumerate(OPS_ISSUES)]
    write(folder, '/rest/agile/1.0/board/21/issue', {'startAt': 0, 'maxResults': 100, 'total': len(issues), 'issues': issues})
    for i in issues:
        write(folder, '/rest/agile/1.0/issue/' + i['key'], i)
    write(folder, '/rest/api/2/user/assignable/search', [rowan])

def main():
    if os.path.isdir(OUT):
        shutil.rmtree(OUT)
    os.makedirs(OUT)
    with open(os.path.join(OUT, 'trackers.txt'), 'w') as f:
        f.write('# The fixture\'s sites, as the plugin\'s trackers.txt names them.\n%s you@acme.example\n' % SITE)
        f.write('https://%s fixture-api-key\n' % TRELLO)
    # The same, with a second account on the Jira site (`ARTOO_TASKS_FIXTURE_SITES=trackers-two.txt`): its own folder.
    with open(os.path.join(OUT, 'trackers-two.txt'), 'w') as f:
        f.write('# Two accounts on one site: the second has its own key, so its own folder and its own token.\n')
        f.write('%s you@acme.example\n' % SITE)
        f.write('%s %s id=%s # Operations\n' % (SITE, SECOND, SECOND_KEY))
        f.write('https://%s fixture-api-key\n' % TRELLO)
    # What connecting a new site answers (`jira::probe`): a site that does not answer, one that refuses the sign-in.
    with open(os.path.join(OUT, 'probes.json'), 'w') as f:
        json.dump({'jira.acme.dev': 'reach', 'acme-payments.atlassian.net': 'token'}, f, indent=2)
    jira(os.path.join(OUT, JIRA))
    files(os.path.join(OUT, JIRA, 'files'))
    second(os.path.join(OUT, SECOND_KEY))
    trello(os.path.join(OUT, TRELLO))

main()
