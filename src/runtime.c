#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef USE_GC
#include <gc.h>
#endif

static int gc_enabled = 0;

void pyrs_runtime_init(int gc_mode) {
  if (gc_mode == 1) { // On
#ifdef USE_GC
    GC_INIT();
    gc_enabled = 1;
#else
    fprintf(
        stderr,
        "Error: PyRS runtime built without GC support but --gc on was used.\n");
    exit(1);
#endif
  } else if (gc_mode == 2) { // Dyn
#ifdef USE_GC
    GC_INIT();
    gc_enabled = 1;
#else
    gc_enabled = 0;
#endif
  } else { // Off
    gc_enabled = 0;
  }
}

void *pyrs_alloc(size_t size) {
  if (gc_enabled) {
#ifdef USE_GC
    return GC_MALLOC(size);
#endif
  }
  return malloc(size);
}

// Basic List Implementation (Placeholder for now)
typedef struct {
  int64_t size;
  int64_t capacity;
  void **data;
} PyList;

PyList *pylist_new(int64_t initial_capacity) {
  PyList *list = (PyList *)pyrs_alloc(sizeof(PyList));
  list->size = 0;
  list->capacity = initial_capacity;
  list->data = (void **)pyrs_alloc(sizeof(void *) * initial_capacity);
  return list;
}

void pylist_append(PyList *list, void *item) {
  if (list->size >= list->capacity) {
    int64_t new_capacity = list->capacity * 2;
    if (new_capacity == 0)
      new_capacity = 4;

    void **new_data = (void **)pyrs_alloc(sizeof(void *) * new_capacity);
    for (int64_t i = 0; i < list->size; i++) {
      new_data[i] = list->data[i];
    }
    list->data = new_data;
    list->capacity = new_capacity;
  }
  list->data[list->size++] = item;
}

void *pylist_get(PyList *list, int64_t index) {
  if (index < 0 || index >= list->size) {
    fprintf(stderr, "IndexError: list index out of range\n");
    exit(1);
  }
  return list->data[index];
}

void pylist_set(PyList *list, int64_t index, void *item) {
  if (index < 0 || index >= list->size) {
    fprintf(stderr, "IndexError: list index out of range\n");
    exit(1);
  }
  list->data[index] = item;
}

#include <inttypes.h>

void pyrs_print_int(int64_t v) { printf("%" PRId64 "\n", v); }

void pyrs_print_float(double v) { printf("%g\n", v); }

void pyrs_print_bool(int v) { fputs(v ? "true\n" : "false\n", stdout); }

void pyrs_print_str(const char *s) { printf("%s\n", s); }

int64_t pylist_len(PyList *list) { return list->size; }

void pylist_print(PyList *list) {
  printf("[");
  for (int64_t i = 0; i < list->size; i++) {
    // This is tricky because we don't know the type of items.
    // For now, assume they are i64 for debugging.
    printf("%ld", (int64_t)list->data[i]);
    if (i < list->size - 1)
      printf(", ");
  }
  printf("]\n");
}

// String helpers
int64_t pyrs_str_len(const char *s) { return (int64_t)strlen(s); }

int64_t pyrs_str_eq(const char *a, const char *b) { return strcmp(a, b) == 0; }
