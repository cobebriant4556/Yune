local runtime = require("@yune/runtime")
local audio = require("@yune/audio")
local function near(actual, expected, tolerance)
    assert(math.abs(actual - expected) < (tolerance or 0.00001), tostring(actual) .. " != " .. tostring(expected))
end
local function make(class)
    local node = Instance.new(class)
    node.Parent = workspace
    return node
end
local function wire(source, target, pin)
    local node = make("Wire")
    node.SourceInstance = source
    node.TargetInstance = target
    node.TargetName = pin or "Input"
    return node
end
local captureIndex = 0
local function capture(frames, chunk)
    captureIndex += 1
    audio.beginCapture({maxSeconds = 2})
    local remaining = frames
    while remaining > 0 do
        local count = math.min(remaining, chunk or frames)
        audio.step(count / 48000)
        remaining -= count
    end
    local pcm = audio.getCaptureBuffer()
    assert(buffer.len(pcm) == frames * 8)
    audio.endCapture("test-results/audio/effects-" .. captureIndex .. ".wav", true)
    return pcm
end
local constant = buffer.create(24000 * 4)
for index = 0, 23999 do
    buffer.writef32(constant, index * 4, 0.5)
end
local impulse = buffer.create(4)
buffer.writef32(impulse, 0, 1)
audio.registerPCM("yune://constant-effects", 48000, 1, constant)
audio.registerPCM("yune://impulse-effects", 48000, 1, impulse)
local classes = audio.getCapabilities().supportedEffects
assert(#classes == 13)
for _, class in ipairs(classes) do
    local player = make("AudioPlayer")
    player.Asset = "yune://constant-effects"
    player.Looping = true
    local effect = make(class)
    local output = make("AudioDeviceOutput")
    local first = wire(player, effect)
    local second = wire(effect, output)
    assert(first.Connected and second.Connected, class)
    assert(effect:GetInputPins()[1] == "Input" and effect:GetOutputPins()[1] == "Output")
    if class == "AudioCompressor" then
        assert(effect:GetInputPins()[2] == "Sidechain")
    end
    effect.Bypass = true
    player:Play()
    local dry = capture(137)
    for index = 0, 273 do
        near(buffer.readf32(dry, index * 4), 0.5)
    end
    effect.Bypass = false
    if class == "AudioFader" then
        effect.Volume = 0.5
    elseif class == "AudioEqualizer" then
        effect.LowGain = -12
    elseif class == "AudioFilter" then
        effect.FilterType = Enum.AudioFilterType.Highpass12dB
    elseif class == "AudioEcho" then
        effect.DelayTime = 0.01
    elseif class == "AudioLimiter" then
        effect.MaxLevel = -12
    end
    local wet = capture(4096)
    local altered = false
    for index = 0, 8191 do
        local sample = buffer.readf32(wet, index * 4)
        assert(sample == sample and math.abs(sample) < math.huge, class .. " produced non-finite samples")
        altered = altered or math.abs(sample - 0.5) > 0.0001
    end
    assert(altered, class .. " did not process its input")
    first:Destroy()
    second:Destroy()
    player:Destroy()
    effect:Destroy()
    output:Destroy()
    audio.step(0)
end
local player = make("AudioPlayer")
player.Asset = "yune://impulse-effects"
local echo = make("AudioEcho")
echo.DelayTime = 0.01
echo.Feedback = 0.5
echo.DryLevel = 0
echo.WetLevel = 0
local output = make("AudioDeviceOutput")
local inputWire = wire(player, echo)
local outputWire = wire(echo, output)
player:Play()
local whole = capture(1920)
near(buffer.readf32(whole, 0), 1)
near(buffer.readf32(whole, 480 * 8), 1)
near(buffer.readf32(whole, 960 * 8), 0.5)
near(buffer.readf32(whole, 1440 * 8), 0.25)
player.TimePosition = 0
audio.resetEffects(echo)
player:Play()
local split = capture(1920, 73)
assert(buffer.tostring(whole) == buffer.tostring(split), "Echo changed with step partitioning")
audio.resetEffects(echo)
local silent = capture(480)
for index = 0, 959 do
    near(buffer.readf32(silent, index * 4), 0)
end
assert(not pcall(function()
    echo.DelayTime = 0 / 0
end))
echo.DelayTime = 100
near(echo.DelayTime, 5)
inputWire:Destroy()
outputWire:Destroy()
echo:Destroy()
player.Asset = "yune://constant-effects"
player.Volume = 0.2
player.Looping = true
player.TimePosition = 0
local compressor = make("AudioCompressor")
compressor.Threshold = -20
compressor.Ratio = 10
compressor.Attack = 0.0001
compressor.Release = 0.01
local detector = make("AudioPlayer")
detector.Asset = "yune://constant-effects"
detector.Volume = 2
detector.Looping = true
local sidechain = wire(detector, compressor, "Sidechain")
wire(player, compressor)
wire(compressor, output)
player:Play()
detector:Play()
local ducked = capture(2048)
near(buffer.readf32(ducked, 2000 * 8), 0.1 * 200 ^ (-0.45))
player.Volume = 0
local noLeak = capture(256)
for index = 0, 511 do
    near(buffer.readf32(noLeak, index * 4), 0)
end
player.Volume = 0.2
sidechain:Destroy()
local restored = capture(9600)
near(buffer.readf32(restored, 9500 * 8), 0.1 * 2 ^ (-0.45))
player:Stop()
detector:Stop()
local filter = make("AudioFilter")
filter.Frequency = 1000
filter.Q = math.sqrt(0.5)
filter.Gain = 6
for _, kind in ipairs(Enum.AudioFilterType:GetEnumItems()) do
    filter.FilterType = kind
    assert(filter.FilterType == kind)
    local gain = filter:GetGainAt(1000)
    assert(gain == gain and math.abs(gain) < math.huge)
end
filter.FilterType = Enum.AudioFilterType.Peak
near(filter:GetGainAt(1000), 6)
filter.FilterType = Enum.AudioFilterType.Lowpass12dB
near(filter:GetGainAt(1000), 10 * math.log10(0.5))
assert(not pcall(function()
    filter.FilterType = Enum.AudioWindowSize.Medium
end))
local pitch = make("AudioPitchShifter")
for _, window in ipairs(Enum.AudioWindowSize:GetEnumItems()) do
    pitch.WindowSize = window
    assert(pitch.WindowSize == window)
    local expected = if window.Value == 0 then 512 elseif window.Value == 2 then 2048 else 1024
    assert(audio.getNodeLatency(pitch).samples == expected)
end
pitch.Bypass = true
assert(audio.getNodeLatency(pitch).samples == 0)
assert(not audio.getCapabilities().robloxDspParity)
print("PASS all 13 effect nodes: real graph processing, bypass, exact echo timing, partition invariance, effect reset, compressor sidechain isolation, filter responses, enum validation and pitch latency")
