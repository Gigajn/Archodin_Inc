-- AAHC (Autonomous AI Holding Company) core schema.
-- Applied by init_db.py; safe to re-run because every statement is IF NOT EXISTS.

-- 1. Company floors & divisions
CREATE TABLE IF NOT EXISTS divisions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    floor_number INTEGER NOT NULL,
    execution_mode TEXT CHECK(execution_mode IN ('DAG_DEV', 'STREAM_TRADE', 'WEBHOOK_CS', 'CRON_BATCH')),
    cron_schedule TEXT,
    is_active BOOLEAN DEFAULT 1,
    room_grid_bounds TEXT -- JSON: {"x": 0, "y": 0, "w": 12, "h": 10}
);

-- 2. Employees (AI agents)
CREATE TABLE IF NOT EXISTS employees (
    id TEXT PRIMARY KEY,
    division_id TEXT REFERENCES divisions(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    role_title TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    model_cluster TEXT CHECK(model_cluster IN ('CLUSTER_CODE', 'CLUSTER_FAST', 'CLUSTER_REASONING', 'CLUSTER_VISION')),
    allowed_tools TEXT NOT NULL, -- JSON array: ["github_search", "docker_bash"]
    assigned_pair TEXT,          -- trading desks only, e.g. "BTC/USDT"
    sprite_asset TEXT NOT NULL,  -- e.g. "char_brown_hair_white_shirt"
    desk_coord_x INTEGER NOT NULL,
    desk_coord_y INTEGER NOT NULL,
    current_state TEXT DEFAULT 'IDLE'
);

-- 3. Workflow steps (chained SOP handoffs)
CREATE TABLE IF NOT EXISTS workflow_steps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    division_id TEXT REFERENCES divisions(id) ON DELETE CASCADE,
    step_order INTEGER NOT NULL,
    from_employee_id TEXT REFERENCES employees(id),
    to_employee_id TEXT REFERENCES employees(id),
    validation_rule TEXT
);

-- 4. Free-tier API pool (9Router state)
CREATE TABLE IF NOT EXISTS router_api_pool (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_name TEXT NOT NULL, -- "groq_1", "openrouter_1", "gemini_1", "ollama_local"
    cluster_type TEXT NOT NULL,
    model_identifier TEXT NOT NULL,
    env_key_name TEXT NOT NULL,
    rpm_limit INTEGER DEFAULT 30,
    is_cooldown BOOLEAN DEFAULT 0,
    cooldown_until DATETIME,
    total_calls_today INTEGER DEFAULT 0
);

-- Seeding idempotency: one lane per provider+cluster+model.
CREATE UNIQUE INDEX IF NOT EXISTS idx_router_lane
    ON router_api_pool (provider_name, cluster_type, model_identifier);

-- 5. Trading ledger (financial audit trail)
CREATE TABLE IF NOT EXISTS trading_ledger (
    trade_id TEXT PRIMARY KEY,
    employee_id TEXT REFERENCES employees(id),
    pair TEXT NOT NULL,
    mode TEXT CHECK(mode IN ('PAPER', 'LIVE')),
    side TEXT CHECK(side IN ('BUY', 'SELL')),
    entry_price REAL NOT NULL,
    stop_loss REAL NOT NULL,
    take_profit REAL NOT NULL,
    position_size REAL NOT NULL,
    risk_manager_approved BOOLEAN DEFAULT 0,
    pnl_realized REAL DEFAULT 0.0,
    status TEXT DEFAULT 'OPEN',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 6. Company skill vault registry
CREATE TABLE IF NOT EXISTS skills_registry (
    skill_id TEXT PRIMARY KEY,
    skill_name TEXT NOT NULL,
    category TEXT NOT NULL,             -- "DEV", "TRADING", "MARKETING", "CS", "CORE"
    source_type TEXT CHECK(source_type IN ('LOCAL_PYTHON', 'MCP_SERVER', 'API_WRAPPER')),
    install_command TEXT,               -- e.g. "pip install crawl4ai"
    skill_md_path TEXT NOT NULL,
    is_verified BOOLEAN DEFAULT 0,      -- must pass sandbox test before real use
    created_by TEXT DEFAULT 'SYSTEM'    -- 'SYSTEM' or 'SKILL_HUNTER_AGENT'
);
