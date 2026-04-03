<script lang="ts">
  import { api } from '$lib/api';

  type PersonResult = {
    entity_id: string;
    display_name: string;
    entity_type: string;
    snippet: string | null;
  };

  type PathStep = {
    person_id: string;
    relationship_label: string;
    direction: string;
  };

  type KinshipPathResponse = {
    path: PathStep[];
    kinship_name: string | null;
  };

  let person1Id: string = '';
  let person2Id: string = '';
  let person1Name: string = '';
  let person2Name: string = '';

  let person1SearchTerm: string = '';
  let person2SearchTerm: string = '';

  let person1SearchResults: PersonResult[] = [];
  let person2SearchResults: PersonResult[] = [];

  let showPerson1Dropdown: boolean = false;
  let showPerson2Dropdown: boolean = false;

  let result: KinshipPathResponse | null = null;
  let isLoading: boolean = false;
  let error: string = '';

  async function searchPersons(searchTerm: string): Promise<PersonResult[]> {
    if (!searchTerm || searchTerm.length < 2) {
      return [];
    }

    try {
      const response = await api.get<{ results: PersonResult[] }>(
        `/api/v1/search?q=${encodeURIComponent(searchTerm)}&entity_type=person&limit=10`
      );
      return response.results;
    } catch (e) {
      console.error('Search error:', e);
      return [];
    }
  }

  async function onPerson1SearchInput(e: Event): Promise<void> {
    const target = e.target as HTMLInputElement;
    person1SearchTerm = target.value;
    showPerson1Dropdown = true;

    if (person1SearchTerm.length >= 2) {
      person1SearchResults = await searchPersons(person1SearchTerm);
    } else {
      person1SearchResults = [];
    }
  }

  async function onPerson2SearchInput(e: Event): Promise<void> {
    const target = e.target as HTMLInputElement;
    person2SearchTerm = target.value;
    showPerson2Dropdown = true;

    if (person2SearchTerm.length >= 2) {
      person2SearchResults = await searchPersons(person2SearchTerm);
    } else {
      person2SearchResults = [];
    }
  }

  function selectPerson1(result: PersonResult): void {
    person1Id = result.entity_id;
    person1Name = result.display_name;
    person1SearchTerm = result.display_name;
    showPerson1Dropdown = false;
    person1SearchResults = [];
  }

  function selectPerson2(result: PersonResult): void {
    person2Id = result.entity_id;
    person2Name = result.display_name;
    person2SearchTerm = result.display_name;
    showPerson2Dropdown = false;
    person2SearchResults = [];
  }

  async function computeRelationship(): Promise<void> {
    if (!person1Id || !person2Id) {
      error = 'Please select both persons';
      return;
    }

    if (person1Id === person2Id) {
      error = 'Please select two different persons';
      return;
    }

    isLoading = true;
    error = '';
    result = null;

    try {
      result = await api.get<KinshipPathResponse>(
        `/api/v1/graph/path/${encodeURIComponent(person1Id)}/${encodeURIComponent(person2Id)}`
      );
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to compute relationship';
    } finally {
      isLoading = false;
    }
  }

  function formatPath(): string {
    if (!result || !result.path || result.path.length === 0) {
      return 'Direct relationship';
    }

    return result.path.map((step, i) => {
      if (i === 0) {
        return step.relationship_label;
      }
      return step.relationship_label;
    }).join(' → ');
  }

  function clearResults(): void {
    person1Id = '';
    person2Id = '';
    person1Name = '';
    person2Name = '';
    person1SearchTerm = '';
    person2SearchTerm = '';
    result = null;
    error = '';
  }
</script>

<div class="page-container">
  <h1>Relationship Calculator</h1>
  <p class="description">Find the relationship between two individuals</p>

  <div class="calculator-grid">
    <!-- Person 1 Picker -->
    <div class="person-picker">
      <label for="person1-search">First Person</label>
      <div class="search-container">
        <input
          id="person1-search"
          type="text"
          placeholder="Search for a person..."
          value={person1SearchTerm}
          on:input={onPerson1SearchInput}
          on:focus={() => (showPerson1Dropdown = true)}
          on:blur={() => setTimeout(() => (showPerson1Dropdown = false), 200)}
          disabled={isLoading}
        />
        {#if showPerson1Dropdown && person1SearchResults.length > 0}
          <div class="dropdown" role="listbox">
            {#each person1SearchResults as result (result.entity_id)}
              <div
                class="dropdown-item"
                role="option"
                on:click={() => selectPerson1(result)}
                on:keydown={(e) => e.key === 'Enter' && selectPerson1(result)}
                tabindex="0"
              >
                <div class="item-name">{result.display_name}</div>
                {#if result.snippet}
                  <div class="item-snippet">{result.snippet}</div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
      {#if person1Id}
        <div class="selected-person">✓ {person1Name}</div>
      {/if}
    </div>

    <!-- Person 2 Picker -->
    <div class="person-picker">
      <label for="person2-search">Second Person</label>
      <div class="search-container">
        <input
          id="person2-search"
          type="text"
          placeholder="Search for a person..."
          value={person2SearchTerm}
          on:input={onPerson2SearchInput}
          on:focus={() => (showPerson2Dropdown = true)}
          on:blur={() => setTimeout(() => (showPerson2Dropdown = false), 200)}
          disabled={isLoading}
        />
        {#if showPerson2Dropdown && person2SearchResults.length > 0}
          <div class="dropdown" role="listbox">
            {#each person2SearchResults as result (result.entity_id)}
              <div
                class="dropdown-item"
                role="option"
                on:click={() => selectPerson2(result)}
                on:keydown={(e) => e.key === 'Enter' && selectPerson2(result)}
                tabindex="0"
              >
                <div class="item-name">{result.display_name}</div>
                {#if result.snippet}
                  <div class="item-snippet">{result.snippet}</div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
      {#if person2Id}
        <div class="selected-person">✓ {person2Name}</div>
      {/if}
    </div>
  </div>

  <!-- Error Message -->
  {#if error}
    <div class="error-message">{error}</div>
  {/if}

  <!-- Buttons -->
  <div class="button-group">
    <button on:click={computeRelationship} disabled={!person1Id || !person2Id || isLoading}>
      {isLoading ? 'Computing...' : 'Calculate Relationship'}
    </button>
    {#if result}
      <button class="secondary-btn" on:click={clearResults}>Clear</button>
    {/if}
  </div>

  <!-- Result Display -->
  {#if result}
    <div class="result-container">
      <h2>Result</h2>
      <div class="relationship-result">
        <div class="person1-name">{person1Name}</div>
        <div class="relationship-text">
          {#if result.kinship_name}
            <strong>is the {result.kinship_name} of</strong>
          {:else}
            <strong>is related to</strong>
          {/if}
        </div>
        <div class="person2-name">{person2Name}</div>
      </div>

      {#if result.path && result.path.length > 0}
        <div class="path-container">
          <h3>Relationship Path</h3>
          <div class="path">{formatPath()}</div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .page-container {
    max-width: 800px;
    margin: 0 auto;
    padding: 2rem;
  }

  h1 {
    font-size: 2rem;
    margin-bottom: 0.5rem;
    color: var(--text-primary, #000);
  }

  .description {
    color: var(--text-secondary, #666);
    margin-bottom: 2rem;
  }

  .calculator-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2rem;
    margin-bottom: 2rem;
  }

  @media (max-width: 768px) {
    .calculator-grid {
      grid-template-columns: 1fr;
    }
  }

  .person-picker {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  label {
    font-weight: 600;
    color: var(--text-primary, #000);
  }

  .search-container {
    position: relative;
  }

  input {
    width: 100%;
    padding: 0.75rem;
    border: 1px solid var(--border-color, #ddd);
    border-radius: 4px;
    font-size: 1rem;
    box-sizing: border-box;
  }

  input:focus {
    outline: none;
    border-color: var(--primary-color, #0066cc);
    box-shadow: 0 0 0 3px rgba(0, 102, 204, 0.1);
  }

  input:disabled {
    background-color: var(--disabled-bg, #f5f5f5);
    cursor: not-allowed;
  }

  .dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    background: white;
    border: 1px solid var(--border-color, #ddd);
    border-top: none;
    border-radius: 0 0 4px 4px;
    max-height: 300px;
    overflow-y: auto;
    z-index: 10;
    box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
  }

  .dropdown-item {
    padding: 0.75rem;
    cursor: pointer;
    border-bottom: 1px solid var(--border-color, #eee);
    transition: background-color 0.2s;
  }

  .dropdown-item:hover {
    background-color: var(--hover-bg, #f0f0f0);
  }

  .dropdown-item:last-child {
    border-bottom: none;
  }

  .item-name {
    font-weight: 500;
    color: var(--text-primary, #000);
  }

  .item-snippet {
    font-size: 0.85rem;
    color: var(--text-secondary, #666);
    margin-top: 0.25rem;
  }

  .selected-person {
    color: var(--success-color, #28a745);
    font-size: 0.9rem;
    margin-top: 0.25rem;
  }

  .error-message {
    background-color: var(--error-bg, #fff3cd);
    border: 1px solid var(--error-border, #ffc107);
    color: var(--error-text, #856404);
    padding: 1rem;
    border-radius: 4px;
    margin-bottom: 1rem;
  }

  .button-group {
    display: flex;
    gap: 1rem;
    margin-bottom: 2rem;
  }

  button {
    flex: 1;
    padding: 0.75rem 1.5rem;
    background-color: var(--primary-color, #0066cc);
    color: white;
    border: none;
    border-radius: 4px;
    font-size: 1rem;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  button:hover:not(:disabled) {
    background-color: var(--primary-hover, #0052a3);
  }

  button:disabled {
    background-color: var(--disabled, #ccc);
    cursor: not-allowed;
  }

  .secondary-btn {
    background-color: var(--secondary-color, #6c757d);
  }

  .secondary-btn:hover {
    background-color: var(--secondary-hover, #5a6268);
  }

  .result-container {
    background-color: var(--card-bg, #f9f9f9);
    border: 1px solid var(--border-color, #ddd);
    border-radius: 8px;
    padding: 2rem;
  }

  .result-container h2 {
    margin-top: 0;
    margin-bottom: 1.5rem;
    color: var(--text-primary, #000);
  }

  .relationship-result {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 2rem;
    padding: 1.5rem;
    background-color: white;
    border-radius: 6px;
  }

  .person1-name,
  .person2-name {
    font-weight: 600;
    font-size: 1.1rem;
    color: var(--text-primary, #000);
    padding: 0.5rem 1rem;
    background-color: var(--highlight-bg, #e3f2fd);
    border-radius: 4px;
  }

  .relationship-text {
    text-align: center;
    min-width: 150px;
  }

  .relationship-text strong {
    color: var(--primary-color, #0066cc);
    font-size: 1rem;
  }

  .path-container {
    margin-top: 1.5rem;
    padding-top: 1.5rem;
    border-top: 1px solid var(--border-color, #ddd);
  }

  .path-container h3 {
    margin-top: 0;
    margin-bottom: 1rem;
    color: var(--text-primary, #000);
  }

  .path {
    padding: 1rem;
    background-color: white;
    border-radius: 4px;
    font-family: monospace;
    font-size: 0.9rem;
    color: var(--text-secondary, #666);
    word-break: break-word;
  }
</style>
