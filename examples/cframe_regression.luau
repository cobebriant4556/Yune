local runtime = require("@yune/runtime")
for _, pair in ipairs({{Vector3.new(0, 0, 8), Vector3.zero}, {Vector3.new(8, 6, 10), Vector3.new(0, 0.5, 0)}, {Vector3.zero, Vector3.yAxis}, {Vector3.zero, -Vector3.yAxis}}) do
    local position, target = pair[1], pair[2]
    local direction = (target - position).Unit
    for _, cf in ipairs({CFrame.lookAt(position, target), CFrame.lookAlong(position, direction), CFrame.new(position, target)}) do
        assert((cf.Position - position).Magnitude < 0.0001)
        assert((cf.LookVector - direction).Magnitude < 0.0001, "look constructor points away from the target")
        assert(cf:PointToObjectSpace(target).Z < 0, "target is behind the camera")
        assert(math.abs(cf.RightVector:Dot(cf.UpVector)) < 0.0001)
        assert((cf.RightVector:Cross(cf.UpVector) + cf.LookVector).Magnitude < 0.0001)
    end
end
print("PASS CFrame lookAt/lookAlong/legacy look constructor, basis orientation and camera target depth")
