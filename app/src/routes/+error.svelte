<script lang="ts">
  import { goto } from '$app/navigation';

  export let error: { message?: string };
  export let status: number;
</script>

<main class="panel">
  <h1>Something went wrong</h1>
  <p>Status: {status}</p>
  <p>{error?.message ?? 'Unexpected route error.'}</p>
  <button type="button" on:click={() => goto('/persons')}>Go Home</button>
</main>

<style>
  .panel {
    background: #ffffff;
    border: 1px solid #fecaca;
    border-radius: 0.75rem;
    padding: 1.25rem;
    color: #7f1d1d;
  }

  button {
    margin-top: 0.75rem;
    background: #dc2626;
    color: #fff;
    border: 0;
    border-radius: 0.45rem;
    padding: 0.5rem 0.75rem;
    cursor: pointer;
  }
</style>