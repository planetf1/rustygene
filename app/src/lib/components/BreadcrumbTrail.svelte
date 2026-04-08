<script lang="ts">
  import { goto } from '$app/navigation';

  type BreadcrumbItem = {
    label: string;
    href?: string;
  };

  export let items: BreadcrumbItem[] = [];
</script>

<nav aria-label="Breadcrumb" class="breadcrumbs">
  <ol>
    {#each items as item, index}
      <li>
        {#if item.href && index < items.length - 1}
          <button type="button" class="crumb-link" on:click={() => goto(item.href ?? '/')}>{item.label}</button>
        {:else}
          <span class={`crumb-current ${index === items.length - 1 ? 'is-current' : ''}`}>{item.label}</span>
        {/if}
      </li>
      {#if index < items.length - 1}
        <li aria-hidden="true" class="sep">/</li>
      {/if}
    {/each}
  </ol>
</nav>

<style>
  .breadcrumbs ol {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
    font-size: 0.82rem;
  }

  .crumb-link {
    border: 0;
    background: transparent;
    color: #5b21b6;
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
    padding: 0;
    font: inherit;
  }

  .crumb-link:hover {
    color: #4c1d95;
  }

  .crumb-current {
    color: #6b5fa0;
  }

  .crumb-current.is-current {
    color: #2d1c4f;
    font-weight: 600;
  }

  .sep {
    color: #9d84d8;
    user-select: none;
  }
</style>
