local runtime = require("@yune/runtime")
workspace.Gravity = 0

local function rig(name, x)
    local model = Instance.new("Model")
    model.Name = name
    model.Parent = workspace
    local root = Instance.new("Part")
    root.Name = "Root"
    root.Anchored = true
    root.CanCollide = false
    root.CFrame = CFrame.new(x, 2, 0)
    root.Parent = model
    local child = Instance.new("Part")
    child.Name = "Child"
    child.CanCollide = false
    child.Parent = model
    local motor = Instance.new("Motor6D")
    motor.Part0 = root
    motor.Part1 = child
    motor.Parent = root
    local controller = Instance.new("AnimationController")
    controller.Parent = model
    local animator = Instance.new("Animator")
    animator.Parent = controller
    return animator, motor, child
end

local function clip(height)
    local sequence = Instance.new("KeyframeSequence")
    sequence.Loop = true
    for _, time in ipairs({0, 1}) do
        local key = Instance.new("Keyframe")
        key.Time = time
        key.Parent = sequence
        local rootPose = Instance.new("Pose")
        rootPose.Name = "Root"
        rootPose.Parent = key
        local pose = Instance.new("Pose")
        pose.Name = "Child"
        pose.Weight = 1
        pose.CFrame = CFrame.new(0, height * time, 0)
        pose.Parent = rootPose
    end
    return sequence
end

local animatorA, motorA, childA = rig("A", -8)
local animatorB, motorB, childB = rig("B", 8)
local trackA = animatorA:LoadAnimation(clip(2))
trackA:Play(0, 1, 1)
runtime.runFrames(15, 1 / 60)
assert(math.abs(motorA.Transform.Y - 0.5) < 0.001)
assert(motorB.Transform.Position.Magnitude < 0.001, "one Animator changed the other rig")
local trackB = animatorB:LoadAnimation(clip(4))
trackB:Play(0, 1, 1)
runtime.runFrames(15, 1 / 60)
assert(math.abs(motorA.Transform.Y - 1) < 0.001)
assert(math.abs(motorB.Transform.Y - 1) < 0.001)
assert(math.abs(childA.CFrame.X + 8) < 0.001 and math.abs(childB.CFrame.X - 8) < 0.001)
assert(#animatorA:GetPlayingAnimationTracks() == 1)
trackA:AdjustWeight(0.2, 0.2)
runtime.runFrames(6, 1 / 60)
assert(math.abs(trackA.WeightCurrent - 0.6) < 0.001)
trackA:Stop(0.2)
runtime.runFrames(12, 1 / 60)
assert(trackA.WeightCurrent < 0.001)
assert(motorA.Transform.Position.Magnitude < 0.001, "stopped track left a stale pose")
assert(trackB.IsPlaying)
trackB:Destroy()
runtime.step(1 / 60)
assert(#animatorB:GetPlayingAnimationTracks() == 0)

local model = Instance.new("Model")
model.Parent = workspace
local parts = {}
for index = 1, 13 do
    local part = Instance.new("Part")
    part.Name = "Chain" .. index
    part.Anchored = index == 1
    part.CanCollide = false
    part.CFrame = CFrame.new(30, 0, 0)
    part.Parent = model
    parts[index] = part
end
local joints = {}
for index = 12, 1, -1 do
    local joint = Instance.new("Motor6D")
    joint.Part0 = parts[index]
    joint.Part1 = parts[index + 1]
    joint.C0 = CFrame.new(0, 1, 0)
    joint.Parent = model
    joints[index] = joint
end
runtime.step(1 / 60)
assert(math.abs(parts[13].CFrame.Y - 12) < 0.001, "joint chain depends on Instance traversal order")
joints[6].Enabled = false
parts[7].CFrame = CFrame.new(30, 40, 0)
runtime.step(1 / 60)
assert(math.abs(parts[7].CFrame.Y - 40) < 0.001, "disabled joint still moved its child")
assert(math.abs(parts[13].CFrame.Y - 46) < 0.001)
print("PASS isolated Animator rigs, weight fades, stopped pose cleanup, long/reordered joint chains, disabled joints")
