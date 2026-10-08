local runtime = require("@yune/runtime")
local audio = require("@yune/audio")
local function near(actual, expected)
    assert(math.abs(actual - expected) < 0.00001, tostring(actual) .. " != " .. tostring(expected))
end
local function make(class, parent)
    local node = Instance.new(class)
    node.Parent = parent or workspace
    return node
end
local function wire(source, target)
    local node = make("Wire")
    node.SourceInstance = source
    node.TargetInstance = target
    return node
end
local captureIndex = 0
local function capture(left, right, source)
    captureIndex += 1
    audio.beginCapture({source = source, maxSeconds = 1})
    audio.step(128 / 48000)
    local pcm = audio.getCaptureBuffer()
    assert(buffer.len(pcm) == 128 * 8)
    for index = 0, 127 do
        near(buffer.readf32(pcm, index * 8), left)
        near(buffer.readf32(pcm, index * 8 + 4), right)
    end
    audio.endCapture("test-results/audio/spatial-" .. captureIndex .. ".wav", true)
end
local pcm = buffer.create(48000 * 4)
for index = 0, 47999 do
    buffer.writef32(pcm, index * 4, 0.25)
end
audio.registerPCM("yune://spatial", 48000, 1, pcm)
local player = make("AudioPlayer")
player.Asset = "yune://spatial"
player.Looping = true
local camera = workspace.CurrentCamera
camera.CFrame = CFrame.new()
local part = make("Part")
part.Anchored = true
part.CFrame = CFrame.new(-10, 0, 0)
local emitter = Instance.new("AudioEmitter")
emitter:SetDistanceAttenuation({[0] = 1, [100] = 1})
emitter.Parent = part
local listener = make("AudioListener", camera)
local output = make("AudioDeviceOutput")
local inputWire = wire(player, emitter)
local outputWire = wire(listener, output)
assert(inputWire.Connected and outputWire.Connected)
assert(#emitter:GetInputPins() == 1 and #emitter:GetOutputPins() == 0)
assert(#listener:GetInputPins() == 0 and #listener:GetOutputPins() == 1)
assert(emitter.PositionType == Enum.EmitterPositionType.Parent)
assert(listener.PositionType == Enum.ListenerPositionType.Parent)
player:Play()
capture(0.25, 0)
part.CFrame = CFrame.new(10, 0, 0)
capture(0, 0.25)
part.CFrame = CFrame.new(0, 0, -10)
capture(0.25 / math.sqrt(2), 0.25 / math.sqrt(2))
part.CFrame = CFrame.new(10, 0, 0)
camera.CFrame = CFrame.Angles(0, math.pi, 0)
capture(0.25, 0)
camera.CFrame = CFrame.new()
emitter:SetDistanceAttenuation({[0] = 1, [20] = 0})
listener:SetDistanceAttenuation({[0] = 0.5})
near(listener:GetAudibilityFor(emitter), 0.25)
near(emitter:GetAudibilityFor(listener), 0.25)
capture(0, 0.0625)
local info = audio.getSpatialInfo(emitter, listener)
near(info.audibility, 0.25)
near(info.pan, 1)
near(info.rightGain, 0.25)
emitter:SetAngleAttenuation({[0] = 1, [180] = 0})
listener:SetAngleAttenuation({[0] = 1, [180] = 0})
near(listener:GetAudibilityFor(emitter), 0.0625)
capture(0, 0.015625)
emitter:SetAngleAttenuation(nil)
listener:SetAngleAttenuation({})
listener:SetDistanceAttenuation(nil)
local copied = emitter:GetDistanceAttenuation()
copied[0] = 0
near(emitter:GetDistanceAttenuation()[0], 1)
assert(not pcall(function()
    emitter:SetDistanceAttenuation({[-1] = 1})
end))
assert(not pcall(function()
    emitter:SetAngleAttenuation({[181] = 1})
end))
assert(not pcall(function()
    emitter:SetDistanceAttenuation({[0] = math.huge})
end))
assert(not pcall(function()
    emitter:SetDistanceAttenuation({["0"] = 1})
end))
assert(not pcall(function()
    local curve = {}
    for index = 0, 400 do
        curve[index] = 1
    end
    emitter:SetDistanceAttenuation(curve)
end))
near(emitter:GetDistanceAttenuation()[0], 1)
emitter.AudioInteractionGroup = "other"
assert(#emitter:GetInteractingListeners() == 0)
assert(#listener:GetInteractingEmitters() == 0)
capture(0, 0)
listener.AudioInteractionGroup = "other"
assert(emitter:GetInteractingListeners()[1] == listener)
assert(listener:GetInteractingEmitters()[1] == emitter)
emitter:SetDistanceAttenuation({[0] = 1})
part.CFrame = CFrame.new(-10, 0, 0)
local attachment = make("Attachment", part)
attachment.CFrame = CFrame.new(5, 0, 0)
emitter.Parent = attachment
capture(0.25, 0)
local positioned = make("Part")
positioned.Anchored = true
positioned.CFrame = CFrame.new(10, 0, 0)
emitter.PositionType = Enum.EmitterPositionType.Instance
emitter.PositionInstance = positioned
assert(emitter.PositionInstance == positioned)
capture(0, 0.25)
emitter.PositionInstance = nil
assert(emitter.PositionInstance == nil)
capture(0, 0)
emitter.PositionType = Enum.EmitterPositionType.Parent
local folder = make("Folder")
emitter.Parent = folder
capture(0, 0)
emitter.Parent = attachment
listener.PositionType = Enum.ListenerPositionType.Instance
listener.PositionInstance = positioned
capture(0.25, 0)
listener.PositionInstance = nil
capture(0, 0)
listener.PositionType = Enum.ListenerPositionType.Parent
emitter.DistanceAttenuationMode = Enum.DistanceAttenuationMode.Linear
emitter.DistanceAttenuationBounds = NumberRange.new(0, 10)
capture(0.125, 0)
emitter.DistanceAttenuationMode = Enum.DistanceAttenuationMode.Custom
capture(0.25, 0)
local loopback = wire(listener, emitter)
assert(not loopback.Connected, "Spatial feedback must be rejected")
loopback:Destroy()
local invalid = wire(player, listener)
assert(not invalid.Connected)
invalid:Destroy()
local pre = make("AudioFader")
pre.Volume = 0.5
local post = make("AudioFader")
post.Volume = 0.5
inputWire.TargetInstance = pre
wire(pre, emitter)
outputWire.TargetInstance = post
wire(post, output)
capture(0.0625, 0)
capture(0.125, 0, listener)
capture(0.125, 0.125, emitter)
local secondEmitter = make("AudioEmitter", positioned)
secondEmitter.AudioInteractionGroup = "other"
secondEmitter:SetDistanceAttenuation({[0] = 1})
wire(pre, secondEmitter)
player.TimePosition = 0
capture(0.0625, 0.0625)
near(player.TimePosition, 128 / 48000)
emitter.AcousticSimulationEnabled = true
listener.AcousticSimulationEnabled = true
audio.step(0)
local warned = false
for _, message in ipairs(audio.getDiagnostics()) do
    if string.find(message, "direct%-path spatial audio only") then
        warned = true
    end
end
assert(warned)
assert(audio.getCapabilities().spatialAudio and not audio.getCapabilities().robloxDspParity)
print("PASS spatial audio: transforms, stereo handedness, curves, groups, explicit positioning, presets, graph feedback, fanout, capture taps and acoustic diagnostics")
