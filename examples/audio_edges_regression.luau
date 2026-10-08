local runtime = require("@yune/runtime")
local audio = require("@yune/audio")
local pcm = buffer.create(480 * 4)
for index = 0, 479 do
    buffer.writef32(pcm, index * 4, 0.25)
end
audio.registerPCM("yune://edge", 48000, 1, pcm)

local player = Instance.new("AudioPlayer")
player.Asset = "yune://edge"
player.Looping = true
local once = 0
local disconnected = 0
player.Looped:Once(function()
    once += 1
end)
local connection = player.Looped:Connect(function()
    disconnected += 1
end)
assert(connection.Connected)
connection:Disconnect()
assert(not connection.Connected)
player:Play()
player.Parent = workspace
assert(player.IsPlaying and player.IsReady, "Reparenting lost the audio state")

assert(#player:GetWaveformAsync(NumberRange.new(0, 0.005), 0) == 0)
assert(#player:GetWaveformAsync(NumberRange.new(1, 2), 8) == 0)
assert(#player:GetWaveformAsync(NumberRange.new(-2, -1), 8) == 0)
assert(#player:GetWaveformAsync(NumberRange.new(0, 0.005), 3.8) == 3)
assert(not pcall(function()
    player:GetWaveformAsync(NumberRange.new(0, 0.005), 0 / 0)
end))

local output = Instance.new("AudioDeviceOutput")
output.Parent = workspace
local function wire(source, target)
    local object = Instance.new("Wire")
    object.SourceInstance = source
    object.TargetInstance = target
    object.Parent = workspace
    return object
end
local first = wire(player, output)
local second = wire(player, output)
audio.beginCapture({maxSeconds = 1})
audio.step(0.01)
assert(buffer.readf32(audio.getCaptureBuffer(), 0) == 0.5, "Multiple incoming wires must sum the same source without double-advancing playback")
assert(player.TimePosition < 0.000001)
audio.endCapture("test-results/audio/fanin.wav", true)
task.wait()
assert(once == 1 and disconnected == 0)
audio.step(0.01)
task.wait()
assert(once == 1)

second.SourceName = "InvalidPin"
assert(not second.Connected)
audio.beginCapture({maxSeconds = 1})
audio.step(0.001)
assert(buffer.readf32(audio.getCaptureBuffer(), 0) == 0.25)
audio.endCapture("test-results/audio/invalid-pin.wav", true)
second:Destroy()

local otherPlayer = Instance.new("Player")
otherPlayer.Parent = game:GetService("Players")
output.Player = otherPlayer
audio.beginCapture({maxSeconds = 1})
audio.step(0.001)
assert(buffer.readf32(audio.getCaptureBuffer(), 0) == 0, "An output for another player must not enter the local master mix")
audio.endCapture("test-results/audio/other-player.wav", true)
output.Player = nil
otherPlayer:Destroy()

local unsupported = Instance.new("AudioDeviceInput")
unsupported.Parent = workspace
local effectWire = wire(unsupported, output)
local ok, err = pcall(function()
    audio.step(0.001)
end)
assert(not ok and string.find(tostring(err), "unsupported audio node"), "Unimplemented audio nodes must not silently pass through")
effectWire:Destroy()
unsupported:Destroy()
audio.step(0)

player:Stop()
player.AudioContent = Content.none
assert(player.Asset == "" and not player.IsReady)
assert(#player:GetWaveformAsync(NumberRange.new(0, 1), 8) == 0)
player:Destroy()
first:Destroy()
audio.step(0)
assert(#audio.getDiagnostics() > 0)
assert(audio.getCapabilities().robloxDspParity == false)
print("PASS headless audio edge cases: waveform ranges, parenting, Once/Disconnect, fan-in, invalid pins, output isolation, and unsupported-node errors")
