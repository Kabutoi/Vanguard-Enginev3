# 03_DecisionsLog.md - Vanguard Engine v3

## [2026-04-19] Initialization & Architectural Selection

### 1. Scene Management: SceneNode Hierarchy
- **Decision**: Implemented a `SceneNode` based hierarchy for the Rust core.
- **Rationale**: User requested `SceneNode` (Task #1) to provide a navigable object-oriented structure for the Agentic layer.
- **Implementation**: Uses `Arc<RwLock<T>>` for thread-safe access within the Task Graph.

### 2. Animation: FABRIK Solver
- **Decision**: Selected FABRIK (Forward And Backward Reaching Inverse Kinematics) for Procedural IK.
- **Rationale**: User requested "fabrik" (Task #4) for higher visual fidelity in prone movement on voxelized terrain compared to CCD.

### 3. Concurrency: Custom Task Graph
- **Decision**: Implementing a custom Task Graph job system as per the Brief (Section 1).
- **Rationale**: To maximize utilization of the i7-14650HX and handle DXR/Physics/AI parallelism.

### 4. Naming Convention: AnkerBreaker Namespace
- **Decision**: All Rust modules follow the `anker_breaker` root namespace.
- **Rationale**: To comply with the Sentinel Directive and ensure project decoupling.

## [2026-04-19] Renderer Implementation

### 1. Graphics API: DX12/DXR via wgpu
- **Decision**: Initialized the `VanguardRenderer` forcing the DX12 backend.
- **Rationale**: To satisfy the requirement for hardware-accelerated ray tracing (DXR) while maintaining binary compatibility.

### 2. Shaders: Data-Driven PBR Uber-Shader
- **Decision**: Implemented `pbr_uber.wgsl` using a uniform-buffer-driven approach.
- **Rationale**: Allows the autonomous agent to modify visual properties (base_color, roughness) via JSON parameter injections without recompiling raw HLSL.

## [2026-04-19] World Management & Agentic Perception

### 1. Grid-Based Partitioning: Voxel Streaming
- **Decision**: Implemented `WorldPartitionManager` to handle dynamic loading of 3D cells.
- **Rationale**: To maintain performance on the 1440p target by only hydrating voxels in the player's immediate vicinity.

### 2. Intelligence: Hybrid Semantic Perception
- **Decision**: Implemented the `PerceptionSystem` which exports a hybrid JSON view of the Scene Graph and Voxelized Grid.
- **Rationale**: Provides the autonomous agent with spatial awareness and semantic context (Actor names, anchors) required for high-level decision making.
