-- Run: lua draw_icon.lua [ridge|nodule|shard|all] [existing directory]
-- Dependencies: Lua, LibreSprite, ImageMagick and GNU base64.
local palette = {
    {23,21,27},
    {48,32,34},
    {81,48,42},
    {121,84,74},
    {164,125,109},
    {198,171,144},
    {134,69,47},
    {216,121,66},
    {33,76,66},
    {53,124,100},
    {86,175,138},
    {139,219,175},
}
local letters = "kdrsSLoOgtTM"
local designs = {
    ridge = {
        "...................kkk",
        "..................kSSsk",
        ".............kk..ksOOrk",
        "............kSskksoOork",
        "..........kkSLssOOorrkk..kk",
        ".........kssssoOorrrkk..kssk",
        ".......kkSstgssorrdkk...krrk",
        "......kSSsttgrrrrddkkk.ksrkk",
        ".....kSLssggrdddkkrsoOOsrrk",
        ".....kssrrsSSsggkdrOOsrddkk",
        "....ksrrrssstTtgkksoorrdddkk",
        "...krrrssstTMTggdkrrrdddkSSsk",
        "..kSsrrsttTTttgddrrrddkSstssk",
        "..ksssstTttggdddrdrdkgttggssk",
        ".ksrssttgggrddorrdddgTttgssdk",
        ".krrssgggrddrroorddgttggsrddk",
        ".kddsrrrdddrsoorrdgttgrrrrdk",
        "..kdsrrrddrSSoorrdggrrkssgkk",
        "..kddrrssggsorrdddrddksstgkrk",
        "...kddgstTgggrdddkddksgttgrook",
        "....kkdgtggrdddoorgggtTTgroOOk",
        ".....kddgddkdroOOrgtTTtgrrrrk",
        ".....kssrddkroOOrrgtMtgksssk",
        "......krddksoOrrddkgttkSssk",
        ".......kdkssorrdksskggrrsk",
        "......kksoOordkksskkddrgk",
        ".....ksoOOrrkkksk.kdgtsk",
        ".....krOorrk..kk..kggk",
        "......krrrk.......kkk",
        ".......kkk",
    },
    nodule = {
        "",
        "",
        "............kkkkkk",
        ".........kkkSLssssk",
        ".......kkSSsssoOrrkkk",
        "......kSLsssttkkroOOOsk",
        "....kkssstTTgkkddrorrrkk",
        "...kSSssstggkkrrddkkosSSk",
        ".....kSLstkkkdddkkkooOOsrrk",
        ".....kssskktTTgtkdkoOOrddkk",
        "....ksrrkgtTTtggtkkoorrrddkk",
        "...krrdkgTTMtggkgkkrrdddkSSsk",
        "..kSsdkgtTMtkkkdddrrrdkSssttk",
        "..ksskgTTtgkdddrddrdkgttkgTtk",
        ".ksrdktTtgkdddkrrdkkgTTkkTtgk",
        ".krrkgTttkkdddkodkkgtTgktTgk",
        ".kddkttgkdddkrrrkkgtTgkgtTgk",
        "..kdkgTgkkddgTtkkkgTgkgtTgkk",
        "..kddgtTtgkgTtgkkktTgkgTtgkrk",
        "...kddgtTTTttgkkddgTtgtTgkrrOk",
        "....kkdgttTtgkddorgdTMMtgkrOOk",
        "...kkrrggggkddroOOrkgttgkrrrk",
        "....kkrrrddkroOOrrdkkggksssk",
        ".....kssrdksoOrrddkkddkSssk",
        "......kSskssoOrrdksskkgTtk",
        ".......ksssoorrdksskkgtgk",
        "........kkssrrrkkrrdddkk",
        "..........kkrrkkkkrrkk",
        "............kk....kk",
        "",
        "",
    },
    shard = {
        "........................kk",
        "......................ksSk",
        ".....................ksOok",
        "..................kkksOork",
        "...............kkkSLsOorrk",
        ".............kkSSsssoorrkk",
        "..........kkkSsstkkoorrkk",
        "........kSSsstggkkrrrdk",
        "......kkSSstgkkkddkkOOsk",
        ".....kssskktTTgtkdkoOOrddkk",
        "....ksrrkgtTTtggtkkoorrrddkk",
        "...krrdkgTTMtggkgkkrrdddkSSsk",
        "..kSsdkgtTMtkkkdddrrrdkSssttk",
        "..ksskgTTtgkdddrddrdkgttkgTtk",
        ".ksrdktTtgkdddkrrdkkgTTkkTtgk",
        ".krrkgTttkkdddkodkkgtTgktTgk",
        ".kddkttgkdddkrrrkkgtTgkgtTgk",
        "..kdkgTgkkddgTtkkkgTgkgtTgkk",
        "..kddgtTtgkgTtgkkktTgkgTtgkrk",
        "...kddgtTTTttgkkddgTtgtTgkrrOk",
        "....kkdgttTtgkddorgdTMMtgkrOOk",
        ".....kddgggkddroOOrkgttgkk",
        ".....kssrddkroOOrrdkkggkk",
        "....krrdddksoOrrddkkddkk",
        "...krdddkssOorrdksskkk",
        "..kssrksoOorrdkssk",
        "..kSssoOOordkksskk",
        "..kssOOorrkkkssk",
        "...krOorrkk..kk",
        "...krrrkk",
        "....kkk",
    },
}
local function draw(rows)
    local layers = {{}, {}, {}, {}}
    for y=1,32 do
        local row = rows[y-1] or ""
        for x=1,32 do
            local mark = row:sub(x,x)
            local slot = mark ~= "" and letters:find(mark, 1, true)
            local pos = (y-1)*32+x
            for _, layer in ipairs(layers) do layer[pos] = 0 end
            if slot then
                layers[1][pos] = 1
                if slot > 1 then layers[2][pos] = slot < 9 and slot or 3 end
                if slot >= 9 then layers[3][pos] = math.min(11,slot) end
                if mark == "L" or mark == "M" or mark == "O" then
                    layers[4][pos] = slot
                    if mark ~= "M" then layers[2][pos] = slot-1 end
                end
            end
        end
    end
    return layers
end

local function quote(value)
    return "'" .. value:gsub("'", "'\\''") .. "'"
end

local function run(command)
    assert(os.execute(command), command)
end

local function write(path, data)
    local file = assert(io.open(path, "wb"))
    assert(file:write(data))
    assert(file:close())
end

-- Empty LibreSprite document with one transparent 32x32 cel on each named layer.
local seed = "5wEAAOClAQAgACAAIAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABnAQAA+vEIAAAAAAAAAAAAHwAAAAQgAwAAAAAAIAAgAAAA/wAAAAcAT3V0bGluZTQAAAAFIAAAAAAAAP8CAAAAAAAAAAAgACAAeJztwQENAAAAwqD3T20PBxQAAADwbhAAAAElAAAABCADAAAAAAAgACAAAAD/AAAADQBDb3BwZXIgbWF0cml4NAAAAAUgAQAAAAAA/wIAAAAAAAAAACAAIAB4nO3BAQ0AAADCoPdPbQ8HFAAAAPBuEAAAASEAAAAEIAMAAAAAACAAIAAAAP8AAAAJAFZlcmRpZ3JpczQAAAAFIAIAAAAAAP8CAAAAAAAAAAAgACAAeJztwQENAAAAwqD3T20PBxQAAADwbhAAAAEiAAAABCADAAAAAAAgACAAAAD/AAAACgBIaWdobGlnaHRzNAAAAAUgAwAAAAAA/wIAAAAAAAAAACAAIAB4nO3BAQ0AAADCoPdPbQ8HFAAAAPBuEAAAAQ=="
local variant, output = arg[1] or "all", arg[2] or "."
assert(variant == "all" or designs[variant], "Use ridge, nodule, shard or all")
run("test -d " .. quote(output))
local temp = os.tmpname()
os.remove(temp)
run("mkdir -m 700 " .. quote(temp))

local function export()
    write(temp .. "/template.b64", seed)
    run("base64 -d " .. quote(temp .. "/template.b64") .. " > " .. quote(temp .. "/template.aseprite"))
    local cli = "SDL_VIDEODRIVER=dummy XDG_CONFIG_HOME=" .. quote(temp)
        .. " HOME=" .. quote(temp) .. " libresprite -b "
    for _, name in ipairs({"ridge", "nodule", "shard"}) do
        if variant == "all" or name == variant then
            local stem = output .. "/cuprum"
            if variant == "all" and name ~= "ridge" then stem = stem .. "-" .. name end
            local colors, pixels = {}, {}
            for i, rgb in ipairs(palette) do
                colors[i] = "[" .. table.concat(rgb, ",") .. "]"
            end
            for i, layer in ipairs(draw(designs[name])) do
                pixels[i] = "[" .. table.concat(layer, ",") .. "]"
            end
            write(temp .. "/paint.js", "const palette = [" .. table.concat(colors, ",") .. "];\n"
                .. "const layers = [" .. table.concat(pixels, ",") .. "];\n" .. [[
const sprite = app.activeSprite;
const color = app.pixelColor;
for (let index = 0; index < layers.length; index++) {
    const image = sprite.layer(index).cel(0).image;
    for (let pos = 0; pos < layers[index].length; pos++) {
        const slot = layers[index][pos];
        if (slot === 0) continue;
        const rgb = palette[slot - 1];
        image.putPixel(pos % 32, Math.floor(pos / 32), color.rgba(rgb[0], rgb[1], rgb[2], 255));
    }
}
sprite.commit();
]])
            run(cli .. quote(temp .. "/template.aseprite") .. " --script " .. quote(temp .. "/paint.js")
                .. " --save-as " .. quote(stem .. ".aseprite"))
            run(cli .. quote(stem .. ".aseprite") .. " --save-as " .. quote(stem .. ".png"))
            run("magick " .. quote(stem .. ".png") .. " -scale 800% " .. quote(stem .. "-preview.png"))
        end
    end
end

local ok, failure = pcall(export)
run("rm -rf -- " .. quote(temp))
if not ok then error(failure) end
