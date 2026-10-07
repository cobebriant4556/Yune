local runtime = require("@yune/runtime")

local replicatedStorage = game:GetService("ReplicatedStorage")
local replicatedFirst = game:GetService("ReplicatedFirst")

local module = Instance.new("ModuleScript")
module.Name = "RuntimeSmokeModule"
module.Source = "return { value = 41 }"
module.Parent = replicatedStorage

local localScript = Instance.new("LocalScript")
localScript.Name = "RuntimeSmokeScript"
localScript.Source = [[
assert(script.Name == "RuntimeSmokeScript")
local module = require(game:GetService("ReplicatedStorage").RuntimeSmokeModule)
_G.YUNE_SCRIPT_CONTEXT_SMOKE = module.value + 1
]]
localScript.Parent = replicatedFirst

runtime.step(1 / 60)
task.wait()
assert(_G.YUNE_SCRIPT_CONTEXT_SMOKE == 42, "ScriptContext/ModuleScript lifecycle did not run")

local ground = Instance.new("Part")
ground.Name = "Ground"
ground.Anchored = true
ground.Size = Vector3.new(30, 1, 30)
ground.CFrame = CFrame.new(0, -3, 0)
ground.Parent = workspace

local falling = Instance.new("Part")
falling.Name = "Falling"
falling.Size = Vector3.new(2, 2, 2)
falling.CFrame = CFrame.new(0, 8, 0)
falling.Parent = workspace

runtime.runFrames(20, 1 / 60)
assert(falling.CFrame.Position.Y < 8, "physics did not apply gravity")

local mass = falling:GetMass()
assert(mass > 0, "dynamic part mass was not initialized")
local velocityBefore = falling.AssemblyLinearVelocity.Y
falling:ApplyImpulse(Vector3.new(0, mass * 100, 0))
runtime.step(1 / 60)
assert(falling.AssemblyLinearVelocity.Y > velocityBefore, "ApplyImpulse did not affect assembly velocity")

local hit = workspace:Raycast(Vector3.new(0, 20, 0), Vector3.new(0, -50, 0))
assert(hit ~= nil, "Workspace:Raycast returned nil")
assert(hit.Instance ~= nil, "raycast did not return an Instance")
assert(hit.Normal.Magnitude > 0.9, "raycast normal was invalid")

local rig = Instance.new("Model")
rig.Name = "Rig"
rig.Parent = workspace

local root = Instance.new("Part")
root.Name = "Root"
root.Anchored = true
root.CanCollide = false
root.Size = Vector3.new(2, 2, 2)
root.CFrame = CFrame.new(10, 2, 0)
root.Parent = rig

local child = Instance.new("Part")
child.Name = "Child"
child.CanCollide = false
child.Size = Vector3.new(1, 2, 1)
child.CFrame = root.CFrame
child.Parent = rig

local motor = Instance.new("Motor6D")
motor.Name = "RootToChild"
motor.Part0 = root
motor.Part1 = child
motor.C0 = CFrame.new()
motor.C1 = CFrame.new()
motor.Transform = CFrame.new(0, 1, 0)
motor.Parent = root

runtime.step(1 / 60)
assert(math.abs(child.CFrame.Position.Y - 3) < 0.05, "Motor6D transform was not solved")

local controller = Instance.new("AnimationController")
controller.Parent = rig
local animator = Instance.new("Animator")
animator.Parent = controller

local sequence = Instance.new("KeyframeSequence")
sequence.Name = "TestSequence"

local key0 = Instance.new("Keyframe")
key0.Time = 0
key0.Parent = sequence
local pose0 = Instance.new("Pose")
pose0.Name = "Child"
pose0.CFrame = CFrame.new(0, 0, 0)
pose0.Weight = 1
pose0.Parent = key0

local key1 = Instance.new("Keyframe")
key1.Time = 1
key1.Parent = sequence
local pose1 = Instance.new("Pose")
pose1.Name = "Child"
pose1.CFrame = CFrame.new(0, 2, 0)
pose1.Weight = 1
pose1.Parent = key1

local track = animator:LoadAnimation(sequence)
track:Play(0, 1, 1)
runtime.runFrames(30, 1 / 60)

assert(track.IsPlaying, "AnimationTrack stopped too early")
assert(track.TimePosition > 0.45 and track.TimePosition < 0.55, "AnimationTrack time did not advance")
assert(motor.Transform.Position.Y > 0.8 and motor.Transform.Position.Y < 1.2, "Animator did not evaluate Motor6D pose")
assert(child.CFrame.Position.Y > 2.8 and child.CFrame.Position.Y < 3.2, "animated Motor6D pose did not reach Part1")

print("Yune runtime smoke passed")
