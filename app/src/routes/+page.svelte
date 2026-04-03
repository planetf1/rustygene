<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';

  onMount(async () => {
    await goto('/persons', { replaceState: true, keepFocus: true, noScroll: true });
  });
</script>

<p>Redirecting to people…</p>
