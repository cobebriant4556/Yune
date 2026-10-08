local runtime = require("@yune/runtime")
local audio = require("@yune/audio")
local process = require("@lune/process")
local serde = require("@lune/serde")
local fs = require("@lune/fs")

local args = process.args
assert(args[1] and args[2], "Usage: yune run examples/audio_spatial_capture.luau input.wav output.wav [seconds] [effectClass|none]")
assert(args[1] ~= args[2], "Input and output paths must be different")
local effectClass = args[4] or "none"
assert(effectClass == "none" or table.find(audio.getCapabilities().supportedEffects, effectClass), "Unknown or unsupported effect class")
local duration = audio.registerAsset("yune://spatial-capture-input", args[1])
local defaultSeconds = duration + (if effectClass == "none" then 0 else 2)
local seconds = tonumber(args[3] or tostring(defaultSeconds))
assert(seconds and seconds == seconds and seconds > 0 and seconds <= 600, "Duration must be greater than 0 and at most 600 seconds")
local frames = math.floor(seconds * audio.sampleRate + 0.5)
assert(frames > 0, "Duration must include at least one output sample")

local function make(class, parent)
    local node = Instance.new(class)
    node.Parent = parent or workspace
    return node
end
local function connect(source, target)
    local wire = make("Wire")
    wire.SourceInstance = source
    wire.TargetInstance = target
    assert(wire.Connected, "Audio graph did not connect")
    return wire
end
local source = make("Part")
source.Name = "AudioSource"
source.Anchored = true
local camera = workspace.CurrentCamera
camera.CFrame = CFrame.new()
local player = make("AudioPlayer")
player.Asset = "yune://spatial-capture-input"
player.Volume = 1
player.PlaybackSpeed = 1
local emitter = make("AudioEmitter", source)
emitter.AcousticSimulationEnabled = false
emitter:SetDistanceAttenuation({[0] = 1, [64] = 0})
local listener = make("AudioListener", camera)
listener.AcousticSimulationEnabled = false
local output = make("AudioDeviceOutput")
local effect = nil
if effectClass ~= "none" then
    effect = make(effectClass)
    if effectClass == "AudioEcho" then
        effect.DelayTime = 0.12
        effect.Feedback = 0.35
        effect.DryLevel = 0
        effect.WetLevel = -9
        effect.RampTime = 0
    elseif effectClass == "AudioReverb" then
        effect.DecayTime = 1.2
        effect.WetLevel = -12
        effect.DryLevel = 0
    elseif effectClass == "AudioPitchShifter" then
        effect.Pitch = 1.25
        effect.WindowSize = Enum.AudioWindowSize.Medium
    elseif effectClass == "AudioFilter" then
        effect.FilterType = Enum.AudioFilterType.Lowpass12dB
        effect.Frequency = 4000
        effect.Q = math.sqrt(0.5)
    end
    connect(player, effect)
    connect(effect, emitter)
else
    connect(player, emitter)
end
connect(listener, output)

local parameters = {}
if effect then
    for _, name in ipairs({"Bypass", "Volume", "Depth", "Mix", "Rate", "Level", "DelayTime", "Feedback", "DryLevel", "WetLevel", "RampTime", "LowGain", "MidGain", "HighGain", "MidRange", "FilterType", "Frequency", "Q", "Gain", "Attack", "Release", "Threshold", "Ratio", "MakeupGain", "MaxLevel", "Pitch", "WindowSize", "DecayTime", "DecayRatio", "Density", "Diffusion", "EarlyDelayTime", "LateDelayTime", "HighCutFrequency", "LowShelfFrequency", "LowShelfGain", "ReferenceFrequency", "Duty", "Shape", "Skew", "Square"}) do
        local ok, value = pcall(function()
            return effect[name]
        end)
        if ok and value ~= nil then
            if typeof(value) == "NumberRange" then
                parameters[name] = {min = value.Min, max = value.Max}
            elseif typeof(value) == "EnumItem" then
                parameters[name] = tostring(value)
            elseif type(value) == "number" or type(value) == "boolean" then
                parameters[name] = value
            end
        end
    end
end

local trajectory = {}
audio.beginCapture({maxSeconds = 600})
player:Play()
local rendered = 0
while rendered < frames do
    local time = rendered / audio.sampleRate
    local x = 12 * math.sin(2 * math.pi * 0.25 * time - math.pi / 2)
    source.CFrame = CFrame.new(x, 0, -8)
    local count = math.min(800, frames - rendered)
    local spatial = audio.getSpatialInfo(emitter, listener)
    table.insert(trajectory, {sample = rendered, frames = count, x = x, y = 0, z = -8, audibility = spatial.audibility, leftGain = spatial.leftGain, rightGain = spatial.rightGain})
    runtime.step(count / audio.sampleRate)
    rendered += count
end
player:Stop()
local report = audio.endCapture(args[2], true)
local events = audio.takeEvents()
for _, event in ipairs(events) do
    event.instance = nil
end
report.input = args[1]
report.sourceDuration = duration
report.requestedSeconds = seconds
report.effectClass = effectClass
report.effectParameters = parameters
report.latencySamples = if effect then audio.getNodeLatency(effect).samples else 0
report.trajectory = trajectory
report.events = events
report.diagnostics = audio.getDiagnostics()
report.robloxDspParity = false
report.spatialModel = "Yune equal-power stereo point source; linear custom attenuation"
report.distanceCurve = {{distance = 0, volume = 1}, {distance = 64, volume = 0}}
report.metadataPath = args[2] .. ".json"
fs.writeFile(report.metadataPath, serde.encode("json", report))
print(serde.encode("json", {path = report.path, metadataPath = report.metadataPath, frames = report.frames, peak = report.peak, rms = report.rms, clippedSamples = report.clippedSamples}))
