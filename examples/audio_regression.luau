local runtime = require("@yune/runtime")
local audio = require("@yune/audio")
local soundService = game:GetService("SoundService")
workspace.Gravity = 0

local function near(actual, expected, tolerance)
    assert(math.abs(actual - expected) <= (tolerance or 0.000001), tostring(actual) .. " != " .. tostring(expected))
end

local pcm = buffer.create(12000 * 4)
for index = 0, 11999 do
    buffer.writef32(pcm, index * 4, 0.5)
end
near(audio.registerPCM("yune://constant", 48000, 1, pcm), 0.25)

local player = Instance.new("AudioPlayer")
player.Parent = workspace
player.Asset = "yune://constant"
assert(player.IsReady)
near(player.TimeLength, 0.25)
assert(player.AssetId == player.Asset)
assert(player.AudioContent.Uri == player.Asset)
assert(not player.IsPlaying)
assert(#player:GetInputPins() == 0)
assert(player:GetOutputPins()[1] == "Output")

local fader = Instance.new("AudioFader")
fader.Volume = 0.5
fader.Parent = workspace
local output = Instance.new("AudioDeviceOutput")
output.Parent = workspace
local analyzer = Instance.new("AudioAnalyzer")
analyzer.Parent = workspace
analyzer.SpectrumEnabled = true

local function wire(source, target)
    local connection = Instance.new("Wire")
    connection.Parent = workspace
    connection.SourceInstance = source
    connection.TargetInstance = target
    return connection
end

local connection = wire(player, fader)
wire(fader, output)
wire(fader, analyzer)
assert(connection.Connected)
assert(#player:GetConnectedWires("Output") == 1)
assert(#fader:GetConnectedWires("Output") == 2)
local cycle = wire(fader, fader)
assert(not cycle.Connected)
cycle:Destroy()

local ended = 0
local looped = 0
local changes = 0
player.Ended:Connect(function()
    ended += 1
end)
player.Looped:Connect(function()
    looped += 1
end)
player:GetPropertyChangedSignal("IsPlaying"):Connect(function()
    changes += 1
end)

player.Volume = 0.5
local base = soundService:GetMixerTime()
local playId = player:Play(base + 144 / 48000)
local stopId = player:Stop(base + 336 / 48000)
assert(type(playId) == "number" and type(stopId) == "number")
assert(player.IsPlaying)
audio.beginCapture({maxSeconds = 1})
audio.step(480 / 48000)
local captured = audio.getCaptureBuffer()
assert(buffer.len(captured) == 480 * 8)
for index = 0, 479 do
    local expected = if index >= 144 and index < 336 then 0.125 else 0
    near(buffer.readf32(captured, index * 8), expected)
    near(buffer.readf32(captured, index * 8 + 4), expected)
end
local report = audio.endCapture("test-results/audio/scheduled.wav")
assert(report.frames == 480 and report.channels == 2)
near(report.duration, 0.01)
near(report.peak, 0.125)
assert(report.clippedSamples == 0)
near(player.TimePosition, 192 / 48000)
assert(not player.IsPlaying)
task.wait()
assert(ended == 0, "Stop must not fire Ended")
assert(changes > 0)

local id = player:Play(audio.getTime() + 10)
assert(player:Cancel(id))
assert(not player:Cancel(id))
assert(not player.IsPlaying)
assert(not player:Cancel())

local position = player.TimePosition
local waveform = player:GetWaveformAsync(NumberRange.new(0, 0.125), 16)
assert(#waveform == 16)
for _, value in ipairs(waveform) do
    near(value, 0.5)
end
near(player.TimePosition, position)

player.TimePosition = 0
player.PlaybackRegion = NumberRange.new(0, 0.125)
player.LoopRegion = NumberRange.new(0.03125, 0.0625)
player.Looping = true
player:Play()
audio.step(3000 / 48000)
near(player.TimePosition, 0.03125)
audio.step(1500 / 48000)
near(player.TimePosition, 0.03125)
task.wait()
assert(looped == 2, "LoopRegion notifications did not match wrap boundaries")
assert(ended == 0)
player:Stop()
position = player.TimePosition
audio.step(0.01)
near(player.TimePosition, position)
player:Play()
audio.step(0.001)
assert(player.TimePosition > position)
player:Stop()

player.Looping = false
player.PlaybackRegion = NumberRange.new(0, 0.03125)
player.TimePosition = 0
player.PlaybackSpeed = 2
local awaited = false
task.spawn(function()
    player.Ended:Wait()
    awaited = true
end)
task.wait()
player:Play()
audio.step(750 / 48000)
task.wait()
assert(ended == 1 and awaited)
near(player.TimePosition, 0.03125)
assert(not player.IsPlaying)

player.PlaybackRegion = NumberRange.new(0)
player.PlaybackSpeed = 0
player.TimePosition = 0
player:Play()
audio.beginCapture({source = player, maxSeconds = 1})
audio.step(0.005)
local frozen = audio.getCaptureBuffer()
assert(buffer.readf32(frozen, 0) == 0)
near(player.TimePosition, 0)
audio.endCapture("test-results/audio/paused.wav")
player:Stop()
player.PlaybackSpeed = 1

local second = Instance.new("AudioPlayer")
second.Parent = workspace
second.AutoLoad = false
second.AssetId = "yune://constant"
assert(not second.IsReady)
second:Play()
assert(second.IsReady and second.IsPlaying)
second:Stop()
second.AutoPlay = true
audio.step(0)
assert(second.IsPlaying)
second:Stop()
audio.step(0)
assert(not second.IsPlaying, "AutoPlay must only run on first entry")
second:Destroy()

player.Volume = 1
fader.Bypass = true
player.TimePosition = 0
player:Play()
audio.beginCapture({maxSeconds = 1})
audio.step(1024 / 48000)
near(buffer.readf32(audio.getCaptureBuffer(), 0), 0.5)
near(analyzer.PeakLevel, 0.5)
near(analyzer.RmsLevel, 0.5)
local spectrum = analyzer:GetSpectrum()
assert(#spectrum > 0 and spectrum[1] > 0)
analyzer.SpectrumEnabled = false
assert(#analyzer:GetSpectrum() == 0)
audio.endCapture("test-results/audio/bypass.wav", true)
player:Stop()

local loadedDuration = audio.registerAsset("yune://roundtrip", "test-results/audio/scheduled.wav")
near(loadedDuration, 0.01)
player.AudioContent = Content.fromUri("yune://roundtrip")
assert(player.IsReady and player.Asset == "yune://roundtrip")
near(player.TimeLength, 0.01)
assert(not pcall(function()
    player.IsReady = false
end))
assert(not pcall(function()
    player.PlaybackSpeed = 0 / 0
end))
assert(not pcall(function()
    audio.step(-1)
end))

local before = audio.getTime()
runtime.runFrames(3, 1 / 60)
near(audio.getTime() - before, 0.05)
near(soundService:GetMixerTime(), audio.getTime())
assert(#audio.takeEvents() > 0)
assert(#audio.takeEvents() == 0)
print("PASS headless AudioPlayer loading, scheduled playback, cancellation, regions, loops, speed, seeking, events/Wait, Wire routing, Fader, Analyzer, PCM/WAV, and runtime clock")
