# => tagged multi-target SDK fixture
import json, sys
from fr_ir.context import MemoryObjectStore
from fr_ir.intent import AgentIntent, IntentNeed
from fr_ir.intent_actions import TaggedIntentAction, TaskChangeOperation
from fr_ir.ir import ProjectRequest, ProjectReference, TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient
client = FrClient(sys.argv[1], executable=sys.argv[2])
handle = client.project('find', 'render', '--signature').at('/rows/0/0')
change = TaskChange(
    [ProjectRequest('caller', ['find', 'caller', '--signature'])],
    [TaskTarget('render', handle, 'replace-body', fragment='{ value.to_uppercase() }\n'),
     TaskTarget('caller', ProjectReference('caller', '/rows/0/0'), 'replace-body',
                fragment='{ render("changed") }\n')],
    {'files-changed':1, 'edits':2, 'changed-operations':2}, ['syntax'],
    TaskDelivery(patch='artifacts/tagged.patch'),
)
store = MemoryObjectStore()
compiled = client.compile(AgentIntent(handle, 'change',
    needs=(IntentNeed('map', 'code_map', '/target'),), packet_limit=65536,
    action=TaggedIntentAction(TaskChangeOperation(change))), store=store)
assert len(compiled.stored_digests) == 2
assert compiled.action_basis.startswith('fraa2:')
result = client.execute_intent(compiled)
assert result.at('/claims/implementation_correspondence') is False
from fr_ir.guide import AgentGoal, GoalSelector, GoalOperation, GoalLimits
from fr_ir.intent_actions import RecipeOperation
goal = AgentGoal('change', selector=GoalSelector(name='render'),
    operation=GoalOperation('recipe', {'verb':'rename'}), checks=('syntax',),
    context=GoalLimits(packet_limit=65536))
guide = client.guide(goal)
recipe = 'schema 1\nrecipe rename-render {\n rename to "display" where name="render" in="src/lib.rs"\n expect matched = 1\n expect refusals = 0\n}\n'
guided = client.compile_guided_intent(guide, TaggedIntentAction(
    RecipeOperation(recipe, ('syntax',), TaskDelivery(patch='artifacts/guide.patch'))))
assert guided.at('/action/review/guide_basis') == guide.at('/basis')
result = client.execute_intent(guided)
from fr_ir.intent_actions import CapabilityOperation
guide = client.guide(AgentGoal('change', selector=GoalSelector(name='display'),
    operation=GoalOperation('capability', {'capability':'rename', 'parameters':{'new_name':'show'}}),
    checks=('syntax',), context=GoalLimits(packet_limit=65536)))
compiled = client.compile_guided_intent(guide, TaggedIntentAction(CapabilityOperation(
    'rename', {'new_name':'show'}, checks=('syntax',), delivery=TaskDelivery(patch='artifacts/capability.patch'))))
result = client.execute_intent(compiled)
from pathlib import Path
source = (Path(sys.argv[1]) / 'src/lib.rs').read_text()
start = source.rindex('show("changed")')
span = {'start':start, 'end':start+4}
guide = client.guide(AgentGoal('change', selector=GoalSelector(name='caller'),
    operation=GoalOperation('capability', {'capability':'inline-call', 'range':span}),
    checks=('syntax',), context=GoalLimits(packet_limit=65536)))
compiled = client.compile_guided_intent(guide, TaggedIntentAction(CapabilityOperation(
    'inline-call', range=span, checks=('syntax',), delivery=TaskDelivery(patch='artifacts/inline.patch'))))
assert compiled.at('/action/review/guide_basis') == guide.at('/basis')
print(json.dumps({'passed':result.passed, 'status':result.at('/workflow/transaction_status')}))
