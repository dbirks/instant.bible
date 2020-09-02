package bible.instant.ui.main

import android.content.Context
import android.os.Build
import android.text.Spanned
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import androidx.core.text.HtmlCompat
import androidx.recyclerview.widget.RecyclerView
import bible.instant.R
import bible.instant.getBookName
import bible.instant.getTranslationLabel
import instantbible.data.Data
import instantbible.service.Service

class VerseResultViewHolder(itemView: View) : RecyclerView.ViewHolder(itemView) {
    val verseTitle: TextView = itemView.findViewById(R.id.verse_title)
    val verseText: TextView = itemView.findViewById(R.id.verse_text)
    val translationsHolder: LinearLayout = itemView.findViewById(R.id.translations)
}

class VerseResultAdapter : RecyclerView.Adapter<VerseResultViewHolder>() {
    var data = listOf<Service.Response.VerseResult>()
        set(value) {
            field = value
            notifyDataSetChanged()
        }

    override fun getItemCount() = data.size

    override fun onBindViewHolder(holder: VerseResultViewHolder, position: Int) {
        val item = data[position]

        holder.verseTitle.text =
            "${getBookName(item.key.book)} ${item.key.chapter}:${item.key.verse}"
        holder.verseText.text =
            getHighlightedText(holder.verseText.context, item)

        for (t in 0 until holder.translationsHolder.childCount) {
            val btn = holder.translationsHolder.getChildAt(t) as Button
            setButtonStyle(
                btn, if (t == item.topTranslationValue) {
                    R.style.ibButtonBold
                } else {
                    R.style.ibButton
                }
            )
        }

        for (t in 0 until holder.translationsHolder.childCount) {
            val btn = holder.translationsHolder.getChildAt(t) as Button
            btn.setOnClickListener {
                holder.verseText.text = getHighlightedText(holder.verseText.context, item, t)
                for (t in 0 until holder.translationsHolder.childCount) {
                    setButtonStyle(
                        holder.translationsHolder.getChildAt(t) as Button,
                        R.style.ibButton
                    );
                }
                setButtonStyle(btn, R.style.ibButtonBold)
            }
        }
    }

    private fun getHighlightedText(
        context: Context,
        item: Service.Response.VerseResult,
        idx: Int = item.topTranslationValue
    ): Spanned {
        return HtmlCompat.fromHtml(
            item.highlightsList.fold(
                item.getText(idx),
                { text, word ->
                    word.toRegex(RegexOption.IGNORE_CASE).replace(text) {
                        "<b><font color='${ContextCompat.getColor(
                            context,
                            R.color.ibTextHighlight
                        )}' >${it.value}</font></b>"
                    }
                }),
            HtmlCompat.FROM_HTML_MODE_LEGACY
        )
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): VerseResultViewHolder {
        val layoutInflater = LayoutInflater.from(parent.context)
        val view = layoutInflater.inflate(R.layout.verse_result_view, parent, false)
        val translationsHolder: LinearLayout = view.findViewById(R.id.translations)

        for (t in 0 until Data.Translation.TOTAL_VALUE) {
            val btn = Button(view.context)
            btn.text = getTranslationLabel(t)
            setButtonStyle(btn, R.style.ibButton)
            btn.background = null
            btn.minWidth = 0
            btn.minimumWidth = 0
            btn.minHeight = 0
            btn.minimumHeight = 0
            btn.setPadding(0, 0, 0, 0)
            val marginParams = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.WRAP_CONTENT,
                LinearLayout.LayoutParams.WRAP_CONTENT
            );
            marginParams.setMargins(0, 0, 10, 0);
            btn.layoutParams = marginParams;
            btn.setTag(R.string.translation_tag, t)
            translationsHolder.addView(btn)
        }

        return VerseResultViewHolder(view)
    }

    private fun setButtonStyle(btn: Button, style: Int) {
        if (Build.VERSION.SDK_INT < 23) {
            btn.setTextAppearance(btn.context, style)
        } else {
            btn.setTextAppearance(style)
        }
    }
}

