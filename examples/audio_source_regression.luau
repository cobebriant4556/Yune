local runtime = require("@yune/runtime")
local audio = require("@yune/audio")
local function near(a, b, tolerance)
    assert(math.abs(a - b) <= (tolerance or 0.000001), tostring(a) .. " != " .. tostring(b))
end
local function make(class)
    local object = Instance.new(class)
    object.Parent = workspace
    return object
end
local function wire(a, b)
    local object = make("Wire")
    object.SourceInstance = a
    object.TargetInstance = b
    return object
end
local count = 0
local function capture(frames)
    count += 1
    audio.beginCapture({maxSeconds = 1})
    audio.step(frames / 48000)
    local result = audio.getCaptureBuffer()
    audio.endCapture("test-results/audio/source-" .. count .. ".wav", true)
    assert(buffer.len(result) == frames * 8)
    return result
end
local source = make("AudioPlayer")
source.Asset = "yune://not-ready-source"
local output = make("AudioDeviceOutput")
local connection = wire(source, output)
assert(source:Play() == 0 and source.IsPlaying and not source.IsReady)
local missing = capture(32)
assert(source.IsPlaying and source.TimePosition == 0)
for i = 0, 63 do
    assert(buffer.readf32(missing, i * 4) == 0)
end
local pcm = buffer.create(4800 * 8)
for i = 0, 4799 do
    buffer.writef32(pcm, i * 8, 0.25)
    buffer.writef32(pcm, i * 8 + 4, -0.25)
end
audio.registerPCM("yune://not-ready-source", 48000, 2, pcm)
local ready = capture(16)
near(buffer.readf32(ready, 0), 0.25)
assert(source.IsReady and source.IsPlaying)
local position = source.TimePosition
assert(source:Stop() == 0)
capture(10)
near(source.TimePosition, position)
source.TimePosition = 0
source.PlaybackRegion = NumberRange.new(1, 2)
source:Play()
local clamped = capture(16)
near(buffer.readf32(clamped, 0), 0.25)
source:Stop()
source.TimePosition = 0
source.PlaybackRegion = NumberRange.new(0.01, 0.010001)
source:Play()
local fuzzy = capture(16)
near(buffer.readf32(fuzzy, 0), 0.25)
near(source.TimePosition, 16 / 48000)
source:Stop()
source.PlaybackRegion = NumberRange.new(0, 0.03125)
source.TimePosition = 0
source:Play()
capture(1500)
assert(not source.IsPlaying and source.TimePosition == 0)
source.PlaybackRegion = NumberRange.new(0)
source.Looping = true
connection:Destroy()
local distortion = make("AudioDistortion")
distortion.Level = 0.5
local inputWire = wire(source, distortion)
local effectWire = wire(distortion, output)
source:Play()
local rational = capture(128)
for i = 0, 127 do
    near(buffer.readf32(rational, i * 8), 0.5)
    near(buffer.readf32(rational, i * 8 + 4), -0.5)
end
inputWire:Destroy()
effectWire:Destroy()
distortion:Destroy()
source:Stop()
local overload = buffer.create(4 * 8)
for i = 0, 3 do
    buffer.writef32(overload, i * 8, if i == 0 then 2 else 0.5)
    buffer.writef32(overload, i * 8 + 4, 0.25)
end
audio.registerPCM("yune://limiter-source", 48000, 2, overload)
source.Asset = "yune://limiter-source"
source.TimePosition = 0
source.Looping = false
local limiter = make("AudioLimiter")
limiter.MaxLevel = 0
limiter.Release = 0.01
wire(source, limiter)
wire(limiter, output)
source:Play()
local limited = capture(4)
near(buffer.readf32(limited, 0), 1)
near(buffer.readf32(limited, 4), 0.25)
near(buffer.readf32(limited, 8), 0.5 / (2 * (0.01 / (1 / 48000 + 0.01))))
near(buffer.readf32(limited, 12), 0.25)
assert(not source.IsPlaying and source.TimePosition == 0)
assert(#audio.getCapabilities().sourceDerivedEffects == 3)
assert(audio.getCapabilities().robloxDspParity == false)
print("PASS source-derived playback readiness/action IDs/end reset/float32 regions and distortion/independent limiter graph outputs")
