core.register_node("generator:stone", {
    description = "Artifact Stone",
    tiles = {"artifact_stone.png"},
    groups = {cracky = 3},
})

core.register_alias("mapgen_singlenode", "generator:stone")
core.set_mapgen_setting("mg_name", "singlenode", true)

local started = false

core.register_on_mods_loaded(function()
    if started then
        return
    end
    started = true
    core.after(0, function()
        core.emerge_area(
            {x = -128, y = -32, z = -128},
            {x = 127, y = 31, z = 127},
            function(_, action, calls_remaining)
                if action == core.EMERGE_ERRORED then
                    core.log("error", "chunklog artifact mapblock generation failed")
                end
                if calls_remaining == 0 then
                    core.after(1, function()
                        core.request_shutdown("chunklog artifact generation complete", false, 0)
                    end)
                end
            end
        )
    end)
end)
