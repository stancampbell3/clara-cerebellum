% Clara FrontDesk - Conversation State Machine
% States: greeting, inquiry, resolution, clarification, farewell

% Valid transitions
transition(greeting, any, inquiry, gather_intent)
transition(inquiry, product_info, resolution, provide_product_info)
transition(inquiry, service_info, resolution, provide_service_info)
transition(inquiry, contact_info, resolution, provide_contact_info)
transition(inquiry, hours_info, resolution, provide_hours_info)
transition(inquiry, unknown, clarification, ask_clarification)
transition(clarification, product_info, resolution, provide_product_info)
transition(clarification, service_info, resolution, provide_service_info)
transition(clarification, contact_info, resolution, provide_contact_info)
transition(clarification, hours_info, resolution, provide_hours_info)
transition(clarification, unknown, clarification, ask_clarification)
transition(clarification, farewell, farewell, say_goodbye)
transition(resolution, followup, inquiry, gather_intent)
transition(resolution, farewell, farewell, say_goodbye)
transition(resolution, any, inquiry, gather_intent)

% Query: current state + intent -> next state + action
next_state(Current, Intent, Next, Action) :- transition(Current, Intent, Next, Action), !
next_state(Current, _, Next, Action) :- transition(Current, any, Next, Action)
