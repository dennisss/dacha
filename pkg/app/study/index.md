

Flash Card Learning App

- Data Storage
    - Flash card set:
        - Name
        - Id
        - Schema
            - List of dimensions
    - Raw flash cards
        - Set Id
        - Order: monotonic order for listing the cards.
        - Id
        - Text Dimension A
        - Text Dimension B
        - Text Dimension C 
    - Active cards:
        - Id
        - Enabled time: When
    - Events
        - User Id
        - Time completed
        - Time spent
        - Primary dimension
        - Hidden dimension
        - 0-1 Success Metric
    - Aggregate card states
        - User Id
        - Card Id
        - Primary Dimension
        - Secondary Dimension
        - Count of attempts
        - Confidence score
            - Probability of getting this card pair correct on the next attempt
            - 1 means that we know it fully
            - Decays over time (exponential decay. ~40% per day initially)
                - Over time, the decay factor can be re-estimated
        - Last Updated
    
- UI
    - Review mode
        - List of 
    - Flash card mode
        - Given: Primary and secondary dimension to correlate.
        - Apply decays
        - Sort all cards by confidence
        - Pick top K (20) with some randomness
        - Show one by one to users
            - User shown dimension A
                - Has option to 'Reveal', 'Skip' , 'End'
            - User clicks on 'Reveal' button and sees answer 
            - User shown answer
                - Has option to 'Rate': Bad (0), 0.25, 0.5, 0.7, 1.0 (Great)
            - Then shown 'Next', 'End' options

- Backends
    - Site: study.dacha.page.
    - Storage: Google Cloud Spanner
    - Authentication: OAuth
    - Compute:
        - Google Cloud Compute Engine running Rust binary
            - Initially test locally.
    - Maintaining consistency:
        - Doesn't matter too much 


Gradually decaying the decay
- If a user hasn't seen a card in a while,
    - And the user outperforms the prediced confidence_score post decay, then that means that the decay is too high and should be lowered
    - Only update if > 30 minutes have passed
    - Weight be how wrong the prediction is.
    - next_attempt_score - (post_decay_confidence - original_confidence)

