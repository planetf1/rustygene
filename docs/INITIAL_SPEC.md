### Initial Technical Specification: AI-Driven Genealogy Engine

**Document Revision Date:** 2026-03-29
**Time:** 21:35

---

#### 1. System Architecture
A decoupled, service-oriented architecture separating strict deterministic storage from probabilistic AI inference.

* **Core Logic & Storage Layer:** Built in Rust. Acts as the absolute source of truth and enforces biological/chronological constraints.
* **Intelligence Layer:** Built in Python using BeeAI/crewAI. Operates as a background sidecar, utilizing LLMs (Gemini) to propose graph mutations.
* **Presentation Layer:** A Tauri application targeting macOS initially, utilizing web technologies for responsive graph rendering.
* **Communication Bridge:** A RESTful API built with Axum (Rust), exposing endpoints for both the UI and the Python agent workers. (Optional optimization: PyO3 bindings for direct core access).

#### 2. Data Model (Probabilistic Graph)
Traditional genealogy databases treat data as binary facts. This system models data as assertions with varying confidence levels.

* **Graph Database:** SurrealDB or PostgreSQL (with graph extensions).
* **Node/Edge Structure:**
    * `Person` (Node)
    * `Fact` / `Event` (Node - e.g., Birth, Census 1881)
    * `Relationship` (Edge - e.g., Parent_Of, Resided_At)
* **Confidence Wrapper Schema:** Every fact and relationship must implement a probabilistic trait:
    ```json
    {
      "entity_id": "uuid",
      "assertion_type": "birth_date",
      "value": "1850-10-14",
      "confidence_score": 0.85,
      "needs_review": true,
      "source_refs": ["uuid-of-1881-census-record"],
      "proposed_by": "agent-inquisitor"
    }
    ```

#### 3. AI Agent Orchestration (`agent.md` & Skills)
Agent configuration and behavioral prompts are defined in markdown files. The agents read from the database, use tools to gather external context, and write proposals to a staging queue.

* **Agent 1: The Validator (Constraint Checking)**
    * *Trigger:* New data entry.
    * *Skill:* Temporal logic reasoning via Gemini.
    * *Action:* Checks for contradictions (e.g., overlapping geographic locations within impossible travel timeframes). Returns a pass/fail and a confidence score.
* **Agent 2: The Discoverer (Upstream Sourcing)**
    * *Trigger:* Scheduled cron or user request for a specific `Person` node.
    * *Skill:* API Connector (FamilySearch, Discovery API).
    * *Action:* Fetches upstream candidate records, scores them against local graph context, and pushes matches > 0.7 confidence to the review queue.
* **Agent 3: The Scraper (Unstructured Data)**
    * *Skill:* Playwright/headless browser automation.
    * *Action:* Navigates target sites (e.g., FreeCEN), extracts raw HTML/OCR text, and uses Gemini to structure it into JSON assertions.

#### 4. API & Connectors
The Rust backend manages external IO and internal routing.

* **Internal API:** REST (JSON) exposing `/graph`, `/staging`, and `/review` endpoints. Openapi specs generated via `utoipa` to auto-build the Python client for the agents.
* **Ingestion Pipeline:** A Rust service utilizing the `ged_io` crate to parse `.ged` files, transforming them into the internal probabilistic schema.
* **External Connectors:**
    * `FamilySearch Connector`: Authenticates and queries the FamilySearch GEDCOM X API.
    * `UK National Archives Connector`: Queries the Discovery API for record metadata and archival references (e.g., RG 14 piece numbers).

#### 5. User Interface (Tauri)
A fast, local-first application designed for heavy data visualization.

* **Framework:** React or Svelte within a Tauri webview.
* **Graph Visualization:** React Flow or Cytoscape.js for interactive relationship mapping.
* **Visual Language:**
    * Solid lines = Verified relationships.
    * Dashed lines = Unverified/AI-proposed relationships.
    * Color gradients (Red to Green) on nodes/edges mapping directly to the `confidence_score`.
* **Review Dashboard:** A dedicated view for the "Human-in-the-Loop" to bulk approve, reject, or modify agent proposals sitting in the staging queue.
