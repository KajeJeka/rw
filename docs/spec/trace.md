# Traceability — PROTECTED file

Maps every claim id to the tests that prove it and to the oracle class of each
test. `tools/check_trace.sh` fails when

- a test names no claim id,
- a test declares no oracle class, or one the trace table does not define,
- a `CONFIRMED AGAINST ORIGINAL BINARY` claim has no test in oracle class 1, 2 or 3,
- the trace table's row for such a claim names a test that does not exist, that
  lives in a file the root `Cargo.toml` does not declare as a `[[test]]` gate
  target, or whose own doc attribute does not carry that claim id,
- a test in the seven gate files is named by no row, or a claim id a test names
  has no row, or
- the map is empty.

## Oracle classes

| Class | Source |
| --- | --- |
| 1 | Bytes of the original executable, read by a separate extraction tool |
| 2 | Closure on real data: exact decoded size, exact consumption, zero trailing bytes |
| 3 | Known plaintext |
| 4 | An independent reference implementation written by the test author |
| 5 | Hand computed vectors in the implementation guide |
| 6 | Algebraic laws |

Not an oracle: the output of the code under test, a snapshot of that output,
Community Edition output, or a value copied from a failure message.

## How to read the map

One row per claim id. The test cell lists `file::test`, semicolon separated; the
class cell lists that test's oracle class in the same order, so a class is always
paired with the test to its left. A claim may appear in more than one gate file.

The class cell is where the oracle class of each test is recorded, and it is the
class the gate reads when it asks whether a `CONFIRMED AGAINST ORIGINAL BINARY`
claim has a witness. The gate then proves that the row is telling the truth: the
file has to be under `tests/gates/`, the root `Cargo.toml` has to declare it as
a `[[test]]` gate target, the test has to exist in that file, and the test's own
doc attribute has to carry the claim id.

A `†` marks a class taken from the module header of the file rather than
restated on the test's own doc comment. It is the class the file declares, not a
claim that this particular test re-declared it.

A `‡` marks a value the ledger owns. The ledger row is the single source of the
number, and the number is changed there first. DAT-021 ‡ carries the 39,038-byte
overrun as the ledger row DAT-C110; `docs/spec/dat.md` and the literal `39_038`
in the corpus test repeat it and neither is the source.

A test that asserts the text of an error message is class 5 only because the
spec fixes that text: `docs/spec/endian.md` and `docs/spec/lzss.md` carry the
exact wording of every variant of `LittleEndianError`, `BigEndianError` and
`LzssError`, and the test author reads the expected string from there. Without
those sections the wording would be a value copied from the code under test,
which the table above lists as not an oracle.

Two witnesses in this map are joint, and the map says so where it matters.
`install_plaintext.rs::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte`
is class 3, known plaintext: one byte-for-byte comparison over 28 real archive
entries constrains the LZSS rules it carries together rather than one at a time.
`install_archive.rs::dat_023_every_entry_decodes_to_its_declared_uncompressed_size`
is class 2: the closure over 20,456 chains constrains the corpus rules it
carries together. Neither test isolates the rule it carries, and neither is
meant to.

Built by reading the seven files under `tests/gates/` and taking each `#[test]`'s
doc comment verbatim: the `claim:` line where there is one, otherwise the claim
ids the doc comment states in prose. Nothing here is inferred.

## Claim to test map

### Byte order layer (`docs/spec/endian.md`)

| Claim | Tests | Oracle class |
| --- | --- | --- |
| END-002 | `install_archive.rs`::dat_025_directory_and_entry_counts_are_exact | 2, 4 |
| END-006 | `synthetic_endian.rs`::synthetic_the_same_bytes_read_one_and_sixteen_million_by_the_two_layers | 5 |
| END-008 | `synthetic_endian.rs`::synthetic_the_same_bytes_read_one_and_sixteen_million_by_the_two_layers | 5 |
| END-010 | `synthetic_endian.rs`::synthetic_no_default_order_and_no_conversion_between_the_layers | 6 |
| END-011 | `synthetic_endian.rs`::synthetic_no_default_order_and_no_conversion_between_the_layers | 6 |
| END-012 | `synthetic_endian.rs`::synthetic_no_default_order_and_no_conversion_between_the_layers | 6 |
| END-013 | `synthetic_endian.rs`::synthetic_no_default_order_and_no_conversion_between_the_layers | 6 |
| END-014 | `synthetic_endian.rs`::synthetic_each_read_consumes_exactly_its_own_width; `synthetic_endian.rs`::synthetic_remaining_falls_by_the_width_of_every_mixed_read; `synthetic_endian.rs`::synthetic_the_asset_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_asset_layer_u16_folds_two_bytes_most_significant_first; `synthetic_endian.rs`::synthetic_the_asset_layer_u32_folds_four_bytes_most_significant_first; `synthetic_endian.rs`::synthetic_the_executable_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_executable_layer_u16_folds_two_bytes_least_significant_first; `synthetic_endian.rs`::synthetic_the_executable_layer_u32_folds_four_bytes_least_significant_first | 5; 6; 5; 5; 5; 5; 5; 5 |
| END-015 | `synthetic_endian.rs`::synthetic_asset_layer_folds_the_most_significant_byte_first; `synthetic_endian.rs`::synthetic_executable_layer_folds_the_least_significant_byte_first; `synthetic_endian.rs`::synthetic_the_asset_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_asset_layer_u16_folds_two_bytes_most_significant_first; `synthetic_endian.rs`::synthetic_the_asset_layer_u32_folds_four_bytes_most_significant_first; `synthetic_endian.rs`::synthetic_the_executable_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_executable_layer_u16_folds_two_bytes_least_significant_first; `synthetic_endian.rs`::synthetic_the_executable_layer_u32_folds_four_bytes_least_significant_first | 5; 5; 5; 5; 5; 5; 5; 5 |
| END-016 | `synthetic_endian.rs`::synthetic_signed_reads_reinterpret_the_folded_bits_rather_than_convert; `synthetic_endian.rs`::synthetic_the_asset_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_executable_layer_i16_reinterprets_two_bytes_as_two_complement | 5; 5; 5 |
| END-017 | `synthetic_endian.rs`::synthetic_every_read_method_refuses_a_buffer_one_byte_short; `synthetic_endian.rs`::synthetic_every_read_method_refuses_an_empty_buffer; `synthetic_endian.rs`::synthetic_the_asset_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_asset_layer_u16_folds_two_bytes_most_significant_first; `synthetic_endian.rs`::synthetic_the_asset_layer_u32_folds_four_bytes_most_significant_first; `synthetic_endian.rs`::synthetic_the_executable_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_executable_layer_u16_folds_two_bytes_least_significant_first; `synthetic_endian.rs`::synthetic_the_executable_layer_u32_folds_four_bytes_least_significant_first | 5; 5; 5; 5; 5; 5; 5; 5 |
| END-018 | `synthetic_endian.rs`::synthetic_a_failed_read_leaves_the_cursor_exactly_where_it_was; `synthetic_endian.rs`::synthetic_every_bit_flip_of_the_asset_layer_answers; `synthetic_endian.rs`::synthetic_every_bit_flip_of_the_executable_layer_answers; `synthetic_endian.rs`::synthetic_every_read_method_refuses_a_buffer_one_byte_short; `synthetic_endian.rs`::synthetic_every_read_method_refuses_an_empty_buffer; `synthetic_endian.rs`::synthetic_the_asset_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_asset_layer_u16_folds_two_bytes_most_significant_first; `synthetic_endian.rs`::synthetic_the_asset_layer_u32_folds_four_bytes_most_significant_first; `synthetic_endian.rs`::synthetic_the_executable_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_executable_layer_u16_folds_two_bytes_least_significant_first; `synthetic_endian.rs`::synthetic_the_executable_layer_u32_folds_four_bytes_least_significant_first | 6; 6; 6; 5; 5; 5; 5; 5; 5; 5; 5 |
| END-019 | `synthetic_endian.rs`::synthetic_seek_takes_any_position_up_to_the_length_and_no_further; `synthetic_endian.rs`::synthetic_skip_only_advances_and_seek_is_the_only_way_back | 5; 6 |
| END-020 | `synthetic_endian.rs`::synthetic_bytes_hands_back_the_callers_own_storage | 6 |
| END-022 | `synthetic_endian.rs`::synthetic_every_error_names_its_offset_its_field_its_origin_and_no_content; `synthetic_endian.rs`::synthetic_the_asset_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_asset_layer_u16_folds_two_bytes_most_significant_first; `synthetic_endian.rs`::synthetic_the_asset_layer_u32_folds_four_bytes_most_significant_first; `synthetic_endian.rs`::synthetic_the_executable_layer_i16_reinterprets_two_bytes_as_two_complement; `synthetic_endian.rs`::synthetic_the_executable_layer_u16_folds_two_bytes_least_significant_first; `synthetic_endian.rs`::synthetic_the_executable_layer_u32_folds_four_bytes_least_significant_first | 5; 5; 5; 5; 5; 5; 5 |
| END-023 | `synthetic_endian.rs`::synthetic_every_error_names_its_offset_its_field_its_origin_and_no_content | 5 |
| END-024 | `synthetic_endian.rs`::synthetic_no_default_order_and_no_conversion_between_the_layers | 6 |
| END-035 | `synthetic_endian.rs`::synthetic_the_same_bytes_read_one_and_sixteen_million_by_the_two_layers; `install_archive.rs`::dat_025_directory_and_entry_counts_are_exact | 5; 2, 4 |

### LZSS decoder (`docs/spec/lzss.md`)

| Claim | Tests | Oracle class |
| --- | --- | --- |
| LZSS-001 | `synthetic_lzss.rs`::synthetic_no_ring_index_above_4095_is_addressable; `synthetic_lzss.rs`::synthetic_ring_constants_match_the_specified_values; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 6; 5, 6; 3 |
| LZSS-002 | `synthetic_lzss.rs`::synthetic_canary_rejects_write_index_4095_zero_and_prefill_00; `synthetic_lzss.rs`::synthetic_fresh_ring_prefills_0_to_4077_and_zeroes_4078_to_4095; `synthetic_lzss.rs`::synthetic_ring_constants_match_the_specified_values; `synthetic_lzss.rs`::synthetic_vector_00000f_decodes_to_eighteen_spaces; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 4; 5; 5, 6; 5; 3 |
| LZSS-003 | `synthetic_lzss.rs`::synthetic_canary_rejects_write_index_4095_zero_and_prefill_00; `synthetic_lzss.rs`::synthetic_output_position_maps_to_ring_slot_position_plus_4078; `synthetic_lzss.rs`::synthetic_ring_constants_match_the_specified_values; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 4; 6; 5, 6; 3 |
| LZSS-004 | `synthetic_lzss.rs`::synthetic_no_ring_index_above_4095_is_addressable | 6 |
| LZSS-005 | `synthetic_lzss.rs`::synthetic_input_exhaustion_mid_control_byte_is_not_an_error; `synthetic_lzss.rs`::synthetic_match_length_range_is_exactly_three_to_eighteen; `synthetic_lzss.rs`::synthetic_vector_00000f_decodes_to_eighteen_spaces; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 5; 5; 5; 3 |
| LZSS-006 | `synthetic_lzss.rs`::synthetic_every_bit_flip_answers_without_panicking; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 6; 3 |
| LZSS-007 | `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 3 |
| LZSS-008 | `synthetic_lzss.rs`::synthetic_match_length_range_is_exactly_three_to_eighteen; `synthetic_lzss.rs`::synthetic_ring_constants_match_the_specified_values; `synthetic_lzss.rs`::synthetic_source_index_is_an_absolute_position_not_a_distance; `synthetic_lzss.rs`::synthetic_vector_token_4e_36_at_output_5000_copies_from_output_4960; `synthetic_lzss.rs`::synthetic_vector_token_75_3f_at_write_index_886_repeats_the_previous_byte; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 5; 5, 6; 5; 5; 5; 3 |
| LZSS-009 | `synthetic_lzss.rs`::synthetic_output_position_maps_to_ring_slot_position_plus_4078; `synthetic_lzss.rs`::synthetic_source_index_is_an_absolute_position_not_a_distance; `synthetic_lzss.rs`::synthetic_vector_token_4e_36_at_output_5000_copies_from_output_4960; `synthetic_lzss.rs`::synthetic_vector_token_75_3f_at_write_index_886_repeats_the_previous_byte; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 6; 5; 5; 5; 3 |
| LZSS-010 | `synthetic_lzss.rs`::synthetic_output_position_maps_to_ring_slot_position_plus_4078; `synthetic_lzss.rs`::synthetic_vector_token_4e_36_at_output_5000_copies_from_output_4960; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 6; 5; 3 |
| LZSS-011 | `synthetic_lzss.rs`::synthetic_overlapping_run_replicates_its_own_output_byte_for_byte; `synthetic_lzss.rs`::synthetic_vector_token_75_3f_at_write_index_886_repeats_the_previous_byte; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 5; 5; 3 |
| LZSS-012 | `synthetic_lzss.rs`::synthetic_fresh_ring_prefills_0_to_4077_and_zeroes_4078_to_4095; `synthetic_lzss.rs`::synthetic_no_ring_index_above_4095_is_addressable; `synthetic_lzss.rs`::synthetic_output_position_maps_to_ring_slot_position_plus_4078; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 5; 6; 6; 3 |
| LZSS-013 | `synthetic_lzss.rs`::synthetic_every_bit_flip_answers_without_panicking; `synthetic_lzss.rs`::synthetic_input_exhaustion_mid_control_byte_is_not_an_error; `synthetic_lzss.rs`::synthetic_vector_00000f_decodes_to_eighteen_spaces; `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size; `install_archive.rs`::lzss_014_a_stream_one_byte_short_and_one_byte_long_both_fail | 6; 5; 5; 2†; 4, 5 |
| LZSS-014 | `synthetic_lzss.rs`::synthetic_every_bit_flip_answers_without_panicking; `synthetic_lzss.rs`::synthetic_input_ending_before_the_declared_size_is_an_error; `synthetic_lzss.rs`::synthetic_input_exhaustion_mid_control_byte_is_not_an_error; `synthetic_lzss.rs`::synthetic_output_would_exceed_the_declared_size_is_an_error; `synthetic_lzss.rs`::synthetic_overrun_by_exactly_one_byte_is_an_error; `synthetic_dat.rs`::synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met; `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size; `install_archive.rs`::lzss_014_a_stream_one_byte_short_and_one_byte_long_both_fail | 6; 5; 5; 5; 5; 5; 2†; 4, 5 |
| LZSS-015 | `synthetic_lzss.rs`::synthetic_mid_pair_back_reference_is_an_error_not_a_loop; `synthetic_dat.rs`::synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met | 5; 5 |
| LZSS-016 | `synthetic_lzss.rs`::synthetic_reset_returns_a_fresh_ring; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 5; 3 |
| LZSS-017 | `synthetic_lzss.rs`::synthetic_vector_token_4e_36_at_output_5000_copies_from_output_4960; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 5; 3 |
| LZSS-018 | `synthetic_lzss.rs`::synthetic_vector_00000f_decodes_to_eighteen_spaces; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 5; 3 |
| LZSS-019 | `synthetic_lzss.rs`::synthetic_vector_token_75_3f_at_write_index_886_repeats_the_previous_byte; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 5; 3 |
| LZSS-020 | `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size | 2† |
| LZSS-021 | `synthetic_lzss.rs`::synthetic_canary_rejects_write_index_4095_zero_and_prefill_00 | 4 |
| LZSS-022 | `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size | 2† |
| LZSS-023 | `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size | 2† |

### DAT archive container (`docs/spec/dat.md`)

| Claim | Tests | Oracle class |
| --- | --- | --- |
| DAT-001 | `synthetic_dat.rs`::synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at; `synthetic_dat.rs`::synthetic_round_trip_reads_every_field; `install_archive.rs`::dat_025_directory_and_entry_counts_are_exact | 5; 4, 5; 2, 4 |
| DAT-002 | `synthetic_dat.rs`::synthetic_round_trip_reads_every_field; `synthetic_dat.rs`::synthetic_every_bit_flip_of_the_root_header_answers_without_panicking; `synthetic_dat.rs`::synthetic_malformed_trees_are_errors; `synthetic_dat.rs`::synthetic_the_directory_list_is_the_root_header_names_in_file_order; `install_archive.rs`::dat_025_directory_and_entry_counts_are_exact | 4, 5; 5, 6; 5; 4, 5; 2, 4 |
| DAT-003 | `synthetic_dat.rs`::synthetic_round_trip_reads_every_field; `synthetic_dat.rs`::synthetic_malformed_trees_are_errors; `synthetic_dat.rs`::synthetic_the_directory_list_is_the_root_header_names_in_file_order; `install_archive.rs`::dat_029_real_lookups_return_the_measured_entries | 4, 5; 5; 4, 5; 2† |
| DAT-004 | `synthetic_dat.rs`::synthetic_round_trip_reads_every_field; `install_archive.rs`::dat_005_block_field_zero_is_the_entry_count_and_field_two_is_sixteen | 4, 5; 2, 4 |
| DAT-005 | `synthetic_dat.rs`::synthetic_block_field_0_is_the_entry_count_not_the_directory_count; `synthetic_dat.rs`::synthetic_round_trip_reads_every_field; `install_archive.rs`::dat_005_block_field_zero_is_the_entry_count_and_field_two_is_sixteen | 6; 4, 5; 2, 4 |
| DAT-006 | `synthetic_dat.rs`::synthetic_block_field_2_is_the_trailer_width_not_the_record_size; `install_archive.rs`::dat_005_block_field_zero_is_the_entry_count_and_field_two_is_sixteen | 6; 2, 4 |
| DAT-007 | `install_archive.rs`::dat_005_block_field_zero_is_the_entry_count_and_field_two_is_sixteen | 2, 4 |
| DAT-008 | `synthetic_dat.rs`::synthetic_round_trip_reads_every_field; `install_archive.rs`::dat_022_every_block_chain_consumes_exactly_its_compressed_size | 4, 5; 2, 4 |
| DAT-009 | `synthetic_dat.rs`::synthetic_round_trip_reads_every_field; `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size | 4, 5; 2† |
| DAT-010 | `synthetic_dat.rs`::synthetic_lookup_is_by_uppercased_path_and_an_absent_path_is_not_an_error; `install_archive.rs`::dat_029_real_lookups_return_the_measured_entries | 6; 2† |
| DAT-011 | `synthetic_dat.rs`::synthetic_dot_directory_name_composes_no_prefix_and_a_named_one_does; `synthetic_dat.rs`::synthetic_round_trip_reads_every_field; `install_archive.rs`::dat_011_path_composition_keys_on_the_directory_name | 5; 4, 5; 2† |
| DAT-012 | `synthetic_dat.rs`::synthetic_tree_order_is_not_physical_order; `install_archive.rs`::dat_029_real_lookups_return_the_measured_entries | 5; 2† |
| DAT-013 | `synthetic_dat.rs`::synthetic_payload_mode_is_the_flags_high_nibble; `synthetic_dat.rs`::synthetic_unknown_payload_nibble_is_an_error_not_a_default; `install_archive.rs`::dat_017_stored_and_chain_entries_split_on_their_compressed_size; `install_archive.rs`::dat_026_payload_mode_counts_are_exact | 6; 5; 2; 2 |
| DAT-014 | `synthetic_dat.rs`::synthetic_payload_mode_is_the_flags_high_nibble | 6 |
| DAT-015 | `synthetic_dat.rs`::synthetic_payload_mode_is_the_flags_high_nibble; `synthetic_dat.rs`::synthetic_stored_entry_takes_the_uncompressed_size_and_rejects_a_nonzero_compressed_size; `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size | 6; 5; 2† |
| DAT-016 | `synthetic_dat.rs`::synthetic_chain_mixes_one_raw_block_and_one_lzss_block_in_stored_order; `synthetic_dat.rs`::synthetic_payload_mode_is_the_flags_high_nibble; `install_archive.rs`::dat_022_every_block_chain_consumes_exactly_its_compressed_size | 4, 5; 6; 2, 4 |
| DAT-017 | `synthetic_dat.rs`::synthetic_stored_entry_takes_the_uncompressed_size_and_rejects_a_nonzero_compressed_size; `install_archive.rs`::dat_017_stored_and_chain_entries_split_on_their_compressed_size | 5; 2 |
| DAT-018 | `synthetic_dat.rs`::synthetic_chain_mixes_one_raw_block_and_one_lzss_block_in_stored_order; `synthetic_dat.rs`::synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met; `install_archive.rs`::dat_020_chain_stats_totals_equal_the_framing_census | 4, 5; 5; 2, 4 |
| DAT-019 | `synthetic_dat.rs`::synthetic_a_chain_of_two_lzss_blocks_gives_each_block_a_fresh_ring; `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size | 4, 5; 2† |
| DAT-020 | `synthetic_dat.rs`::synthetic_chain_mixes_one_raw_block_and_one_lzss_block_in_stored_order; `install_archive.rs`::dat_020_chain_stats_totals_equal_the_framing_census; `install_archive.rs`::dat_022_every_block_chain_consumes_exactly_its_compressed_size | 4, 5; 2, 4; 2, 4 |
| DAT-021 ‡ | `synthetic_dat.rs`::synthetic_a_stored_region_is_bounded_by_the_compressed_size_not_the_uncompressed_one; `install_archive.rs`::dat_021_the_stored_extent_is_bounded_by_the_compressed_size | 5; 2†, 4 |
| DAT-022 | `synthetic_dat.rs`::synthetic_chain_block_headers_account_for_the_compressed_size_exactly; `install_archive.rs`::dat_022_every_block_chain_consumes_exactly_its_compressed_size | 5; 2, 4 |
| DAT-023 | `synthetic_dat.rs`::synthetic_declared_output_larger_than_the_payload_produces_is_an_error; `synthetic_dat.rs`::synthetic_stream_one_byte_short_and_one_byte_long_are_each_errors; `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size; `install_plaintext.rs`::every_candidate_decodes_to_its_declared_uncompressed_size | 5; 5; 2†; 6 |
| DAT-024 | `synthetic_dat.rs`::synthetic_declared_output_larger_than_the_payload_produces_is_an_error; `synthetic_dat.rs`::synthetic_stream_one_byte_short_and_one_byte_long_are_each_errors; `synthetic_dat.rs`::synthetic_every_error_variant_names_what_went_wrong; `synthetic_dat.rs`::synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met; `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size | 5; 5; 5; 5; 2† |
| DAT-025 | `install_archive.rs`::dat_025_directory_and_entry_counts_are_exact; `install_archive.rs`::the_install_fingerprint_is_exact; `install_archive.rs`::the_harness_rejects_a_foreign_install | 2, 4; 2; 2† |
| DAT-026 | `install_archive.rs`::dat_026_payload_mode_counts_are_exact | 2 |
| DAT-027 | `install_archive.rs`::dat_020_chain_stats_totals_equal_the_framing_census | 2, 4 |
| DAT-028 | `install_plaintext.rs`::dat_028_candidate_discovery_lists_all_thirty_one_names; `install_plaintext.rs`::dat_028_three_patched_copies_differ_at_their_first_byte; `install_plaintext.rs`::dat_028_three_loose_files_differ_in_size_and_are_excluded; `install_plaintext.rs`::dat_028_every_candidate_is_accounted_for_in_exactly_one_bucket; `install_plaintext.rs`::every_candidate_decodes_to_its_declared_uncompressed_size; `install_archive.rs`::dat_028_the_three_loose_size_exclusions_are_exact | 3†; 3†; 3†; 3†; 6; 2, 5 |
| DAT-029 | `install_archive.rs`::dat_029_real_lookups_return_the_measured_entries; `install_plaintext.rs`::dat_028_candidate_discovery_lists_all_thirty_one_names; `install_plaintext.rs`::dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte | 2†; 3†; 3 |
| DAT-030 | `synthetic_dat.rs`::synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at; `synthetic_dat.rs`::synthetic_the_archive_reports_the_label_it_was_parsed_under; `synthetic_dat.rs`::synthetic_every_error_variant_names_what_went_wrong; `install_archive.rs`::dat_023_every_entry_decodes_to_its_declared_uncompressed_size | 5; 5; 5; 2† |
| DAT-033 | `synthetic_dat.rs`::synthetic_a_zero_entry_block_and_a_zero_length_name_are_accepted | 5 |
| DAT-034 | `synthetic_dat.rs`::synthetic_a_zero_entry_block_and_a_zero_length_name_are_accepted | 5 |

### Executable tables (`docs/spec/exe-tables.md`)

| Claim | Tests | Oracle class |
| --- | --- | --- |
| EXETBL-001 | `synthetic_exe.rs`::synthetic_the_build_gate_refuses_an_image_that_is_not_the_reference_build; `synthetic_exe.rs`::synthetic_the_crate_own_digest_is_the_published_digest_of_the_published_vectors; `synthetic_exe.rs`::synthetic_the_digest_is_checked_before_any_header_field_is_read; `install_exe_tables.rs`::install_gate_refuses_two_short_buffers_before_touching_a_header; `install_exe_tables.rs`::install_gate_rejects_an_executable_with_one_bit_flipped; `install_exe_tables.rs`::install_reference_build_is_admitted_and_carries_the_expected_digest; `install_exe_tables.rs`::install_every_exe_error_carries_the_payload_and_the_message_the_spec_fixes | 5; 5; 5; 5; 5; 1; 1, 5 |
| EXETBL-002 | `synthetic_exe.rs`::synthetic_every_section_declares_a_zero_virtual_size_and_containment_still_works; `synthetic_exe.rs`::synthetic_a_section_with_a_zero_raw_pointer_is_never_file_backed; `synthetic_exe.rs`::synthetic_a_read_that_leaves_its_section_is_refused; `synthetic_exe.rs`::synthetic_a_read_that_falls_in_no_section_is_refused; `synthetic_exe.rs`::synthetic_the_address_at_the_end_of_a_section_belongs_to_no_section; `synthetic_exe.rs`::synthetic_the_seven_tables_are_read_from_the_documented_file_offsets; `synthetic_exe.rs`::synthetic_an_image_truncated_before_the_section_table_is_refused; `synthetic_exe.rs`::synthetic_an_image_with_a_bad_signature_is_refused; `synthetic_exe.rs`::synthetic_an_image_with_a_pe32_plus_optional_header_is_refused; `synthetic_exe.rs`::synthetic_an_image_with_an_impossible_section_count_is_refused; `synthetic_exe.rs`::synthetic_every_error_variant_prints_its_specified_text; `synthetic_exe.rs`::synthetic_every_refusal_carries_the_payload_the_spec_names; `synthetic_exe.rs`::synthetic_the_section_boundary_cases_are_refused_or_accepted_as_documented; `synthetic_exe.rs`::synthetic_the_crate_constants_are_the_specified_addresses; `install_exe_tables.rs`::install_a_section_with_a_zero_raw_pointer_is_never_file_backed; `install_exe_tables.rs`::install_bit_flips_across_a_dgroup_slice_are_answered_never_panicked; `install_exe_tables.rs`::install_pe32_plus_optional_header_is_refused; `install_exe_tables.rs`::install_read_va_is_total_over_length_and_over_address; `install_exe_tables.rs`::install_read_va_returns_every_table_window_from_dgroup_raw_data; `install_exe_tables.rs`::install_every_exe_error_carries_the_payload_and_the_message_the_spec_fixes | 5; 5; 5; 5; 5; 5; 5; 5; 5; 5; 5; 5; 5; 5; 1; 6; 5; 5; 1; 1, 5 |
| EXETBL-003 | `synthetic_exe.rs`::synthetic_the_hit_location_penalties_are_the_nine_documented_values; `install_exe_tables.rs`::install_hit_location_penalties_are_the_nine_documented_values | 5; 1 |
| EXETBL-004 | `synthetic_exe.rs`::synthetic_the_critter_critical_table_is_810_records_of_28_bytes; `synthetic_exe.rs`::synthetic_every_critical_record_accessor_reads_its_own_distinct_field; `install_exe_tables.rs`::install_critical_success_critters_are_810_records_of_28_bytes; `install_exe_tables.rs`::install_critter_critical_first_and_last_records_carry_the_measured_bytes | 5; 5; 1; 1 |
| EXETBL-005 | `synthetic_exe.rs`::synthetic_the_player_critical_table_is_54_records_of_28_bytes; `install_exe_tables.rs`::install_critical_success_player_is_54_records_of_28_bytes; `install_exe_tables.rs`::install_player_critical_first_and_last_records_carry_the_measured_bytes | 5; 1; 1 |
| EXETBL-006 | `synthetic_exe.rs`::synthetic_the_critical_failure_table_is_seven_rows_of_five_documented_values; `install_exe_tables.rs`::install_critical_failure_is_seven_rows_of_five_values | 5; 1 |
| EXETBL-007 | `synthetic_exe.rs`::synthetic_the_stat_limits_are_38_records_of_six_fields; `synthetic_exe.rs`::synthetic_every_stat_limit_accessor_reads_its_own_distinct_field; `install_exe_tables.rs`::install_stat_limits_are_thirty_eight_records_of_six_fields_with_a_zero_field_zero | 5; 5; 1 |
| EXETBL-008 | `synthetic_exe.rs`::synthetic_the_player_stat_limits_are_five_records_with_the_documented_ranges; `install_exe_tables.rs`::install_player_stat_limits_are_five_records_with_the_documented_ranges | 5; 1 |
| EXETBL-009 | `synthetic_exe.rs`::synthetic_the_xp_thresholds_are_the_twenty_one_triangular_numbers_indexed_by_level; `install_exe_tables.rs`::install_xp_table_holds_the_twenty_one_triangular_numbers_indexed_by_level | 5; 1 |
| EXETBL-010 | `synthetic_exe.rs`::synthetic_the_critter_critical_table_ends_where_the_player_critical_table_begins; `install_exe_tables.rs`::install_critter_critical_table_ends_where_the_player_table_begins | 6; 1 |
| EXETBL-011 | `synthetic_exe.rs`::synthetic_the_stat_limits_end_where_the_player_stat_limits_begin; `install_exe_tables.rs`::install_stat_limits_end_where_the_player_stat_limits_begin | 6; 1 |
| EXETBL-012 | `synthetic_exe.rs`::synthetic_the_player_stat_limits_end_where_the_xp_table_begins; `install_exe_tables.rs`::install_player_stat_limits_end_where_the_xp_table_begins | 6; 1 |
| EXETBL-013 | `install_exe_tables.rs`::install_xp_for_level_one_is_one_while_the_same_four_bytes_fold_big_endian_to_16777216 | 1 |
| EXETBL-014 | `install_exe_tables.rs`::install_stat_limit_record_sixteen_has_a_minimum_of_zero_and_not_minus_sixty | 1 |
| EXETBL-015 | `synthetic_exe.rs`::synthetic_the_experience_of_a_level_is_the_threshold_scaled_by_one_thousand; `synthetic_exe.rs`::synthetic_a_negative_level_is_refused_before_the_table_is_indexed; `install_exe_tables.rs`::install_xp_table_holds_the_twenty_one_triangular_numbers_indexed_by_level | 5; 5; 1 |
| EXETBL-016 | `install_exe_tables.rs`::install_stat_limits_are_thirty_eight_records_of_six_fields_with_a_zero_field_zero | 1 |
| EXETBL-017 | `install_exe_tables.rs`::install_stat_limits_are_thirty_eight_records_of_six_fields_with_a_zero_field_zero | 1 |

## Claims with no test

20 of the 110 claim ids in `docs/spec/*.md` are cited by no gate test. Each reason below is why, not an excuse.

| Claim | Why there is no test |
| --- | --- |
| DAT-031 | UNVERIFIED. A construction rule: uppercasing belongs to the caller, not to the parser. The behaviour is graded under DAT-010 (`synthetic_dat.rs::synthetic_lookup_is_by_uppercased_path_and_an_absent_path_is_not_an_error`), which cites the wrong id for this mandate. Covered in substance, cited under DAT-010. |
| DAT-032 | UNVERIFIED. Same construction rule as DAT-031: exact match against the stored uppercase path. Graded under DAT-010, cited there. |
| END-001 | UNVERIFIED, lowered by the traceability gate. A scoping claim: two layers exist, asset big-endian and executable little-endian. The class 1 witness is `install_exe_tables.rs::install_xp_for_level_one_is_one_while_the_same_four_bytes_fold_big_endian_to_16777216`, which folds the same four bytes through both readers and gets 1 and 16,777,216. That test carries EXETBL-013 and not END-001, so no oracle class 1, 2 or 3 test carries this claim. Restoring the tag needs one claim id in that test's doc attribute. |
| END-003 | CONFIRMED IN COMMUNITY EDITION ONLY. The asset-layer extension list. No crate in this phase reads `.PRO`, `.FRM`, `.MAP`, `.GCD`, `.AAF` or `.INT`; the asset layer has no reader yet, so nothing can grade the list. |
| END-004 | UNVERIFIED, lowered by the traceability gate. Which tables live in the executable layer. `install_exe_tables.rs` proves every one of them under EXETBL-003 to EXETBL-009 and never cites END-004. The class 1 witness is `install_exe_tables.rs::install_read_va_returns_every_table_window_from_dgroup_raw_data`, which carries EXETBL-002. Restoring the tag needs one claim id in that test's doc attribute. |
| END-005 | UNVERIFIED, lowered by the traceability gate. The XP thresholds table is 21 i32 at 0x5082D0. The class 1 witness is `install_exe_tables.rs::install_xp_table_holds_the_twenty_one_triangular_numbers_indexed_by_level`, which proves exactly this under EXETBL-009 and EXETBL-015. Restoring the tag needs one claim id in that test's doc attribute. |
| END-007 | UNVERIFIED, lowered by the traceability gate. The unscaled triangular numbers and the 1,000 scale. The class 1 witness is `install_exe_tables.rs::install_xp_table_holds_the_twenty_one_triangular_numbers_indexed_by_level`, which proves the table under EXETBL-015; the scale itself is ledger row EXETBL-C821, tagged UNVERIFIED there because the bytes do not say who multiplies. Restoring the tag needs one claim id in that test's doc attribute. |
| END-009 | UNVERIFIED, lowered by the traceability gate. A crate-boundary mandate: no crate outside the two endian crates reads an integer by another route. Its natural test is a dependency-graph or forbidden-import scan over `crates/*`, which is not a gate test in this phase, and no oracle class in the guide describes a fact about the source tree. |
| END-021 | UNVERIFIED. A construction rule: one error enum per crate, a closed set, matched with no wildcard arm. `synthetic_endian.rs` grades what the variants carry (END-022, END-023) but never the per-crate enum count. Coverable by a `const _: () = ...` assertion; not written. |
| END-025 | CONFIRMED IN COMMUNITY EDITION ONLY. The `.ACM` 14-byte little-endian header. No crate in this phase reads an audio file; the spec itself queues the byte check. |
| END-026 | CONFIRMED IN COMMUNITY EDITION ONLY. The `.MVE` 18-byte magic. No crate in this phase reads a movie; the spec itself queues the byte check. |
| END-027 | UNVERIFIED. `.LIP` layout, with the community reader diverging at 0x14. The spec directs the reader to isolate it and not read a shipped `.LIP` until the layout is settled. No test can exist before the layout is. |
| END-028 | UNVERIFIED, lowered by the traceability gate. The 30,051-byte big-endian save header. No save crate exists in this phase, so no gate test reads a save file; the bytes are in the install and a class 2 test through the `fo1-tests` harness could witness them once a save reader exists. |
| END-029 | UNVERIFIED, lowered by the traceability gate. Save block 20, the world map, 899 + 105 + 24 closing on 1,028. No save crate exists in this phase, so no gate test reads a save file; the bytes are in the install and a class 2 test through the `fo1-tests` harness could witness them once a save reader exists. |
| END-030 | UNVERIFIED. Save block 23, whose element type the spec explicitly refuses to fix. The spec directs the reader to a function in the executable first. A test would have to pick a width the spec forbids picking. |
| END-031 | CONFIRMED IN COMMUNITY EDITION ONLY. The port save format, which is this project's own file. No save crate and no port format exists in this phase. |
| END-032 | VERSION DIFFERENCE IDENTIFIED. A statement that the guide's single big-endian rule is superseded by correction C1. It records a decision, not a behaviour, so it has nothing to grade. |
| END-033 | VERSION DIFFERENCE IDENTIFIED. A statement that the older `.INT` note is discarded and must not be implemented. Same shape: a decision, not a behaviour. |
| LZSS-024 | UNVERIFIED. An interface claim, and one the spec itself records as stated twice with two spellings. `synthetic_lzss.rs` exercises both spellings but cites LZSS-013/014/005 for the behaviour. A compile-time signature assertion would cover it; not written. |
| LZSS-025 | Community Edition agrees on the same value. No Community Edition artefact is in scope and no CE source is quoted, so nothing here can grade it. |

## Claims with an open gap

Six claims are listed here. Two carry a test, so each has a row in the map above, and the gap below is what that test does not settle. Four carry no test row at all, and the row names the witness that is missing. Each reason is what is still missing, not an excuse.

| Claim | What is still missing |
| --- | --- |
| DAT-021 | Confirmed. The rule is graded by `synthetic_dat.rs::synthetic_a_stored_region_is_bounded_by_the_compressed_size_not_the_uncompressed_one`, and the corpus is graded by `install_archive.rs::dat_021_the_stored_extent_is_bounded_by_the_compressed_size`, which walks all 25,243 entries of both archives, asserts that 0 of them have a stored extent past the end of their archive, and asserts that exactly the 7 entries the spec names have a decoded extent past it, each with its exact overrun in bytes. The corpus test is written. The ledger holds the measured consequence (DAT-C120: 0 entries past EOF; DAT-C110: the uncompressed bound fails by 39038). DAT-C110 is the single source of the 39,038-byte overrun; `docs/spec/dat.md` and the test's `39_038` literal repeat it. What is still missing is a witness that the three copies of the number agree: no test compares the spec prose or the test literal against DAT-C110. |
| DAT-030 | Confirmed. Three members of the interface are graded, and the corpus grades the whole surface over the real archives. `synthetic_dat.rs::synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at` asserts the whole `DatError::Truncated { at }` value against the byte position the root header layout fixes, `synthetic_dat.rs::synthetic_the_archive_reports_the_label_it_was_parsed_under` asserts that the archive carries the label `parse` was handed, and `synthetic_dat.rs::synthetic_every_error_variant_names_what_went_wrong` asserts that every variant of the error type renders a message of its own that names the quantity it is about. What is still missing is a compile-time signature assertion of the kind `synthetic_endian.rs` carries in its `const _` block: the three tests pin the error's byte position, the origin label and each variant's message, not the field names and types of the whole shape of the interface. |
| DAT-031 | UNVERIFIED. Missing witness: a test that cites the claim. The behaviour is graded under DAT-010 by `synthetic_dat.rs::synthetic_lookup_is_by_uppercased_path_and_an_absent_path_is_not_an_error`, which cites the wrong id for this mandate. |
| DAT-032 | UNVERIFIED. Missing witness: a test that cites the claim. Same construction rule as DAT-031: exact match against the stored uppercase path, graded under DAT-010 and cited there. |
| DAT-033 | UNVERIFIED, added by an orchestrator ruling. No byte evidence establishes a zero field 0 as malformed. The behaviour is asserted by `synthetic_dat.rs::synthetic_a_zero_entry_block_and_a_zero_length_name_are_accepted`, which annotates `claim: DAT-033, DAT-034`. |
| DAT-034 | UNVERIFIED, added by an orchestrator ruling. Same status and same citation as DAT-033. |

Three more claims carry a test and still have no class 1, 2 or 3 witness: END-014,
END-015 and END-016. Their rows name the class 5 and class 6 synthetic endian
tests, which pin the widths, the fold order and the signed reinterpretation
against hand computed vectors, and the executable layer's class 1 gate file does
not cite them. `docs/spec/endian.md` carries the reason and names the class 1
test that would settle each.

## Orchestrator-ruled claims

DAT-031, DAT-032, DAT-033 and DAT-034 were added to `docs/spec/dat.md` by an orchestrator ruling rather than by a measurement. All four carry `UNVERIFIED`: DAT-031 and DAT-032 are construction rules about who uppercases and how a lookup matches, DAT-033 and DAT-034 record that a zero field 0 and a zero-length name parse, with no byte evidence that either is malformed. `check_trace.sh` therefore does not demand a class 1, 2 or 3 test for them, and none is written. The behaviour of DAT-033 and DAT-034 is asserted by `synthetic_dat.rs::synthetic_a_zero_entry_block_and_a_zero_length_name_are_accepted`, which annotates `claim: DAT-033, DAT-034`.

## Totals

| Subsystem | Claims in spec | Mapped | No test |
| --- | --- | --- | --- |
| END | 34 | 18 | 16 |
| LZSS | 25 | 23 | 2 |
| DAT | 34 | 32 | 2 |
| EXETBL | 17 | 17 | 0 |
| **all** | **110** | **90** | **20** |

137 tests across seven gate files: `synthetic_endian.rs` 22, `synthetic_lzss.rs` 18, `synthetic_dat.rs` 23, `synthetic_exe.rs` 31, `install_archive.rs` 14, `install_plaintext.rs` 6, `install_exe_tables.rs` 23.
