# Grammar repetition review

The review covers all 89 entries in the repetition inventory after the first two
grammar-sharing passes, using `00d5550` as the baseline. It changed 45 of the
original productions, reused one existing production in an additional caller,
and retained 43 original forms.

Homogeneous lists use a typed item and preserve written order, separators, and
the item’s scope. Several lists now accept additional lengths for edited or
synthesized ASTs. Dice tables and search phrases with length-specific callers
keep their existing lengths. The two top-of-library shuffle readings share a
prefix and retain their separate final bindings.

The same screen for four or more references to one symbol finds 89 forms
before the pass and 48 after it. The shared lists introduce 74 net named
syntax rules and 363 net bytes of syntax source.
Two of the remaining hits are now shared rules for the four-noun union and
the repeated top-of-library choices. Their original callers shared one fixed
length. The improvement is in how the grammar expresses repeated operands.

The immutable allowed population produced identical complete results for 32,755
faces and 33,357 readings. The affected print audit preserved exact keyed
printing and semantic roundtrips across 91 faces and all 93 readings. Raphael,
Fiendish Savior now defaults to an equivalent singular filter union. Its Oracle
wording is still available with a key. The dash-separated die table retains six
rows because a trial with variable lengths reached the reading limit on
Six-Sided Die.

## Changed productions

| Original rule | Result |
| --- | --- |
| `value_shared_other_four_subtypes` | The count takes an ordered subtype list and retains the bounded unordered print inverse. |
| `search_filter_four_alternatives` | The search and ability-source filters share the four nouns and their union. |
| `search_five_choices` | The five-choice search shares the three-choice suffix and retains its five choices. |
| `search_filter_four_keywords` | The keyword alternatives use a typed list with the existing Hexproof family predicate. |
| `search_four_choices` | The four-choice search shares the three-choice suffix with its original length. |
| `level_range` | The level interval and stat operands feed one stage with an optional ability tail. |
| `level_range_abilities` | The optional ability tail shares the same range stage declaration. |
| `craft_four_types` | Craft materials are an ordered list of independently described types. |
| `choose_card_types_five` | The serial card-type choices compose from vocabulary operands. |
| `choose_color_type_options_five` | Each serial option retains its own color and creature type. |
| `clause_return_four_graveyard_kinds` | The additional kinds each contribute a selection and its own move. |
| `four_permanent_options` | The serial permanent choices capture the eligible populations independently. |
| `choose_unchosen_names_shared` | The unchosen name choices preserve every name as a CardsInOracle operand. |
| `event_player_filter_choice` | The event subject shares the permanent-option list and retains its own chooser binding. |
| `dice_table_program` | Two shared pairs and a final row preserve the five keyword rows of the six-sided die. |
| `five_optional_battlefield_targets` | Each optional battlefield target contributes its own set and announcement. |
| `instructions_repeat_top_library_four_shuffle` | Both readings share the four choices and moves. This shuffle reads every recorded choice. |
| `instructions_repeat_top_library_last_choice_shuffle` | The shared prefix retains the final choice receipt for this shuffle reading. |
| `instructions_opponents_choose_four_sacrifice_rest` | The opponent uses the existing four-category phrase with its own eligibility and sacrifice. |
| `entry_counter_list_four` | Each entry counter contributes a separate event modification in written order. |
| `added_type_list` | Each listed subtype is converted independently to a creature type. |
| `additional_count_pump_changes` | Both additional-pump phrases share the stat changes and differ by the counting operand. |
| `random_grant_seven_options` | Each random ability option carries its own keyword or protection. |
| `additional_counted_pump_changes` | The named-card count uses the same additional-pump phrase and operation. |
| `four_kind_union` | The four-way union shares the final three spelled subtypes. |
| `program_sentence_grouped_die_table` | Two shared triplets preserve the six rows and their common roll receipt. |
| `clause_color_vote` | The ballot preserves each color word and the controller-first voting order. |
| `objects_four_targets` | The serial targets retain independent slots and a single union. |
| `five_subtype_union` | Two subtype operands extend the same three-member suffix to five. |
| `return_optional_four` | The common optional-return list handles four independent graveyard targets. |
| `counter_stack_union_four` | The stack filter has a shared serial tail and one target slot for the whole union. |
| `graveyard_four_subtypes` | The subtype union retains the bounded unordered print inverse. |
| `counter_source_four_nouns` | The ability-source filter shares the four-noun union with the search filter. |
| `filter_four_and_heads` | The four-filter phrase extends the existing three-filter suffix. |
| `recipients_each_without_four_keywords` | Each excluded keyword contributes a separate negated filter within the conjunction. |
| `card_kind_union_four` | The four-way card union shares the suffix and retains its and/or spelling. |
| `seven_subtype_union` | The subtype list retains the vocabulary for its original members. |
| `return_optional_five` | The common optional-return list handles five independent graveyard targets. |
| `target_six_subtype_union` | The list of subtypes stays inside one target declaration. |
| `controlled_kind_union` | The controlled-kind union retains its ordinary and companion spellings. |
| `random_green_token_choice` | Each random choice retains the full token creation action. |
| `copy_override_filter` | The copy override shares the subtype suffix and retains its unordered print inverse. |
| `spell_color_four` | The four-color union shares the three-color suffix and preserves order. |
| `attack_qualified_plural` | Each keyword contributes its own predicate to the union below the attack filter. |
| `four_plural_subtypes_and_or` | The plural subtype union preserves the and/or separator and the entry event scope. |

## Retained productions

| Original rule | Reason |
| --- | --- |
| `condition_control_four` | Each filter uses one shared player in its own control condition. Factoring the list would need an inverse that recovers that player across the conditions. |
| `dig_reveal_four_each` | Each selection excludes earlier selections. The reveal and move collect four specific receipts. |
| `instructions_players_exile_until_die_cast` | The three dice outcomes have different comparison shapes and their casting caps use the corresponding rows. |
| `spell_variable_x_do_all_above` | The thresholds gate distinct programs. The final branch repeats the preceding effects. |
| `search_pair_reveal_die_table` | The range endpoints and open-ended threshold gate different placements of the searched cards. |
| `clause_chosen_target_population_copy` | The written repetitions identify the chosen object, its copy population, and the copy source. |
| `tiered_shared_death_return_pt` | Each tier binds its own mana cost and stats around a shared targeted grant. |
| `instructions_choose_each_then_sacrifice_other` | The four article choices feed a fixed selection count and a sacrifice from an independently filtered population. |
| `chosen_target_five_counters` | The first placement has a variable amount and the later placements share the recipient of that first placement. |
| `spell_five_sweeping_modes` | The sweeping modes have different payloads. The maximum choice count belongs to this fixed modal text. |
| `spell_compare_opponent_choices_exile` | The repeated filters refer to the original controller selection, the opponent comparison, and the unchosen set. |
| `clause_choose_one_of_four_counters` | Both spellings tie every choice arm to one affected set. A shared semantic inverse would need to prove the set agrees. |
| `choose_target_counter_four_kinds` | The branches assign the first counter and the optional kinds to two independently introduced recipients. |
| `chosen_targets_coin_destroy` | Each decision maker chooses their own target. The later nouns identify which target the coin outcome destroys. |
| `coin_life_stakes_process` | A single agreed stake drives gain and loss in distinct branches and is updated by each repeat. |
| `choice_four_categories` | The existing four-category list now also serves the opponent instruction. |
| `controlled_target_graveyard_swap` | The battlefield and graveyard targets have separate filters. Their repeated names identify the simultaneous sacrifice and return operands. |
| `instructions_x_three_branches_all_above` | The numeric occurrences include both thresholds and a scry amount. The final branch repeats the distinct earlier effects. |
| `clause_tempt_copy_target` | The repeated noun identifies a chosen copy source through multiple token-creation scopes. |
| `choose_two_gix_modes` | The modes have distinct syntax and scoped payloads surrounding a shared mode count. |
| `repeated_player_return_types` | The first selection introduces the target player. Subsequent returns reference that player and have their own movement receipts. |
| `program_die_token_table` | Each row has a different threshold shape. Later rows refer to the first row’s token recipe. |
| `instead_sacrifice_either_both` | The original and replacement texts correlate two filters with different choice and effect structure. |
| `instead_controlled_creatures_event_subtype` | The two stat pairs and durations belong to the original event and its replacement branch. |
| `instructions_conditional_split_damage` | Each condition branch has separate amounts for the player or planeswalker and the creature. |
| `independent_static_four` | The independent gates share a scalar source or attachment, including across quoted abilities with their own holders. |
| `independent_static_five` | The five-entry form has the same shared-subject proof as the four-entry form. |
| `conditional_keywords_4` | Each keyword grant combines a common population with a distinct quality. A shared inverse must recover both operands. |
| `conditional_keywords_5` | The five-entry form has the same common-filter proof as the four-entry form. |
| `counter_placement_population` | The broad plural filter can itself contain unions. An open outer list would introduce new groupings in a static restriction. |
| `clause_plural_pump_unless_payment` | The base pump and unpaid follow-up have separate stat pairs and full resolution timing. |
| `creature_pump_conditional_untap` | The first pump marks the population for the conditional untap and additional snapshot. |
| `program_sentence_target_die_ranges` | Two distinct die ranges scope the optional library move and required hand return. |
| `activated_hand_adjusted_die_table` | The adjusted roll thresholds, discard cutoff, and draw amount occupy distinct operand positions. |
| `triggered_four_unchosen_life_modes` | Each unchosen mode has its own scope and repeats a shared life payment before its specific body. |
| `plural_four` | A trial shared a suffix with the three-item list. That added a helper to a frequently used filter path for little structural gain. The original count stays. |
| `plural_five` | The five-item spelling uses a different lexical class that includes the marked Bats form. |
| `plural_five_and_or` | The fourth slot uses a singular filter where the other slots use plural filters. |
| `population_exception` | The exception begins with a singular head and has a separate plural tail. |
| `card_filter_four_and` | The graveyard arrival qualifier uses card words in a fixed phrase and has a narrower lexical domain than the general filter list. |
| `random_shared_token_characteristics` | The three token options have independent stats, subtypes, and holders. The final quoted ability supplies the closing punctuation. |
| `kicker_target_additional_count_pump` | The additional pump is scoped to the kicked branch and counts cards in a different population from the ordinary pump. |
| `event_four_alternative_filter` | The fixed list is under a single tap event. The broad filter grammar can already consume alternatives within each member. |
| `paired_saddle_crew_referenced_event` | The two later filters have to agree with their respective saddle and crew sources. |

## Re-evaluation

The largest coherent remaining group is the shared-subject and shared-filter
static abilities. Work on that group needs a bidirectional binding proof that
retains separate conditions and preserves the holders of quoted abilities.
Use a targeted semantic review for that group and for correlated counter
choices. The remaining fixed numeric slots, mode bodies, and reference phrases
gain little from another repetition screen.
