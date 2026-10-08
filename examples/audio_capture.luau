local runtime = require("@yune/runtime")
local audio = require("@yune/audio")
local process = require("@lune/process")
local serde = require("@lune/serde")

local args = process.args
assert(args[1] and args[2], "Usage: yune run examples/audio_capture.luau input.wav output.wav [seconds] [speed]")
local duration = audio.registerAsset("yune://capture-input", args[1])
local speed = tonumber(args[4] or "1")
assert(speed and speed == speed and speed > 0 and speed <= 20, "Speed must be greater than 0 and at most 20")
local seconds = tonumber(args[3] or tostring(duration / speed))
assert(seconds and seconds == seconds and seconds > 0 and seconds <= 600, "Duration must be greater than 0 and at most 600 seconds")

local player = Instance.new("AudioPlayer")
player.Asset = "yune://capture-input"
player.PlaybackSpeed = speed
player.Parent = workspace

local output = Instance.new("AudioDeviceOutput")
output.Parent = workspace
local wire = Instance.new("Wire")
wire.SourceInstance = player
wire.TargetInstance = output
wire.Parent = workspace
assert(wire.Connected, "Audio output wire did not connect")

audio.beginCapture({maxSeconds = 600})
player:Play()
local frames = math.floor(seconds * 60)
runtime.runFrames(frames, 1 / 60)
local remainder = seconds - frames / 60
if remainder > 0 then
    runtime.step(remainder)
end
player:Stop()
local result = audio.endCapture(args[2], true)
print(serde.encode("json", result))
