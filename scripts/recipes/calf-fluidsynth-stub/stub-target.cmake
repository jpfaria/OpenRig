# Injected into Calf's configure (CMAKE_PROJECT_INCLUDE) by build_calf: Calf
# links a bare `fluidsynth`, which the linker resolves to the system's shared
# library whenever one is installed. An imported target of that name makes
# CMake link the no-op stub archive by full path instead.
if(NOT TARGET fluidsynth)
    add_library(fluidsynth STATIC IMPORTED)
    set_target_properties(fluidsynth PROPERTIES IMPORTED_LOCATION "${CALF_FLUIDSYNTH_STUB}")
endif()
